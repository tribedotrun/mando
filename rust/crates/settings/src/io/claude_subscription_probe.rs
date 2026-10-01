//! Live account-bound subscription metadata from a registered Desktop session.
use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::{de::DeserializeOwned, Deserialize};
use std::path::Path;

pub(crate) struct SubscriptionSnapshot {
    pub account_uuid: String,
    pub plan_name: String,
    pub billing_interval: Option<String>,
    pub status: String,
    pub renews_at: Option<i64>,
    pub ends_at: Option<i64>,
    pub ends_before: Option<String>,
    pub next_charge_date: Option<String>,
}

// Upstream DTOs allow extra fields because Claude owns these APIs. Required
// metadata remains strict; missing/invalid dates never become guessed renewals.
#[derive(Deserialize)]
struct Account {
    uuid: String,
}

#[derive(Deserialize)]
struct OrganizationRef {
    uuid: String,
}

#[derive(Deserialize)]
struct Organization {
    uuid: String,
    rate_limit_tier: String,
    analytics_subscription_plan: Option<String>,
    plan_display_name: Option<String>,
}

#[derive(Deserialize)]
struct SubscriptionDetails {
    status: String,
    billing_interval: Option<String>,
    next_charge_at: Option<String>,
    plan_ending_at: Option<String>,
    plan_ending_before: Option<String>,
    next_charge_date: Option<String>,
}

#[derive(Deserialize)]
struct UsageHistory {
    samples: Vec<UsageSample>,
}

#[derive(Deserialize)]
struct UsageSample {
    t: i64,
    org: String,
}

pub(crate) async fn probe(path: &Path, expected_account: &str) -> Result<SubscriptionSnapshot> {
    anyhow::ensure!(
        is_uuid(expected_account),
        "Invalid registered Claude account identity"
    );
    let configured = super::claude_desktop_profile::account_uuid(path)?;
    anyhow::ensure!(configured.as_deref() == Some(expected_account), "Claude Desktop profile account has changed; relink this credential before refreshing billing");
    let cookie = super::claude_desktop_auth::cookie_header(path).await?;
    let client = global_net::claude_desktop_client()
        .map_err(|error| network_error("client setup", &error))?;
    let account: Account = get(&client, &cookie, "/api/account", "account identity").await?;
    anyhow::ensure!(
        account.uuid == expected_account,
        "Claude Desktop session belongs to a different account; sign in to the linked account"
    );
    let organizations: Vec<OrganizationRef> = get(
        &client,
        &cookie,
        "/api/organizations",
        "organization membership",
    )
    .await?;
    let organization = select_organization(path, &organizations)?;
    let metadata: Organization = get(
        &client,
        &cookie,
        &format!("/api/organizations/{organization}"),
        "plan",
    )
    .await?;
    anyhow::ensure!(
        metadata.uuid == organization,
        "Claude returned a different organization for the linked profile"
    );
    let plan_name = plan_name(&metadata)?;
    if plan_name == "Free" {
        return Ok(SubscriptionSnapshot {
            account_uuid: account.uuid,
            plan_name,
            billing_interval: None,
            status: "free".to_owned(),
            renews_at: None,
            ends_at: None,
            ends_before: None,
            next_charge_date: None,
        });
    }
    let billing: SubscriptionDetails = get(
        &client,
        &cookie,
        &format!("/api/organizations/{organization}/subscription_details"),
        "billing",
    )
    .await?;
    validate_label(&billing.status, "subscription status")?;
    if let Some(interval) = &billing.billing_interval {
        validate_label(interval, "billing interval")?;
    }
    Ok(SubscriptionSnapshot {
        account_uuid: account.uuid,
        plan_name,
        billing_interval: billing.billing_interval,
        status: billing.status,
        renews_at: timestamp(billing.next_charge_at.as_deref(), "renewal")?,
        ends_at: timestamp(billing.plan_ending_at.as_deref(), "plan end")?,
        ends_before: date_only(
            billing.plan_ending_before.as_deref(),
            billing.plan_ending_at.is_none(),
            "plan ending before",
        )?,
        next_charge_date: date_only(
            billing.next_charge_date.as_deref(),
            billing.next_charge_at.is_none(),
            "next charge date",
        )?,
    })
}

fn select_organization(profile: &Path, organizations: &[OrganizationRef]) -> Result<String> {
    let hint = match std::fs::read(profile.join("plan-usage-history.json")) {
        Ok(bytes) => {
            anyhow::ensure!(
                bytes.len() <= 4 * 1024 * 1024,
                "Claude Desktop usage history is too large"
            );
            let history: UsageHistory = serde_json::from_slice(&bytes)
                .map_err(|error| schema_error("Desktop usage history", &error))?;
            history
                .samples
                .into_iter()
                .max_by_key(|sample| sample.t)
                .map(|sample| sample.org)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Could not read Claude Desktop organization hint"),
    };
    let selected = match hint {
        Some(hint) => {
            anyhow::ensure!(organizations.iter().any(|org| org.uuid == hint), "Claude Desktop organization is no longer available to this account; reopen this profile");
            hint
        }
        None if organizations.len() == 1 => organizations[0].uuid.clone(),
        None => anyhow::bail!("Claude account has no unambiguous organization; open this Desktop profile and select its plan"),
    };
    anyhow::ensure!(
        is_uuid(&selected),
        "Claude returned an invalid organization identity"
    );
    Ok(selected)
}

async fn get<T: DeserializeOwned>(
    client: &wreq::Client,
    cookie: &str,
    endpoint: &str,
    label: &str,
) -> Result<T> {
    let mut cookie_header = wreq::header::HeaderValue::from_str(cookie)
        .map_err(|error| anyhow::anyhow!("Claude Desktop session header is invalid: {error}"))?;
    cookie_header.set_sensitive(true);
    let response = client
        .get(format!("https://claude.ai{endpoint}"))
        .header(wreq::header::COOKIE, cookie_header)
        .header(wreq::header::ACCEPT, "application/json")
        .header(wreq::header::REFERER, "https://claude.ai/settings/billing")
        .send()
        .await
        .map_err(|error| network_error(label, &error))?;
    let status = response.status().as_u16();
    if status == 401 || (300..400).contains(&status) {
        anyhow::bail!("Claude {label} request HTTP {status}: Desktop login expired; reopen this profile and sign in");
    }
    if status == 403 {
        anyhow::bail!("Claude {label} request HTTP 403: session or browser verification rejected; reopen this Desktop profile");
    }
    anyhow::ensure!(
        status == 200,
        "Claude {label} request failed: HTTP {status}"
    );
    const LIMIT: usize = 1024 * 1024;
    if let Some(length) = response.content_length() {
        anyhow::ensure!(
            length <= LIMIT as u64,
            "Claude {label} response exceeded size limit"
        );
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| network_error(label, &error))?;
        anyhow::ensure!(
            bytes.len() + chunk.len() <= LIMIT,
            "Claude {label} response exceeded size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|error| schema_error(label, &error))
}

fn network_error(label: &str, error: &wreq::Error) -> anyhow::Error {
    // Do not attach wreq errors: debug/source chains can contain request headers.
    let reason = if error.is_timeout() {
        "timed out"
    } else if error.is_connect() {
        "could not connect"
    } else {
        "transport failed"
    };
    anyhow::anyhow!("Claude {label} request {reason}")
}

fn schema_error(label: &str, error: &serde_json::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "Claude {label} response schema invalid ({:?}, line {}, column {})",
        error.classify(),
        error.line(),
        error.column()
    )
}

fn timestamp(value: Option<&str>, label: &str) -> Result<Option<i64>> {
    value
        .map(|value| {
            let date =
                time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                    .map_err(|error| {
                        anyhow::anyhow!("Claude {label} timestamp is invalid: {error}")
                    })?;
            i64::try_from(date.unix_timestamp_nanos() / 1_000_000)
                .with_context(|| format!("Claude {label} timestamp is outside the supported range"))
        })
        .transpose()
}

fn date_only(value: Option<&str>, needed: bool, label: &str) -> Result<Option<String>> {
    if !needed {
        return Ok(None);
    }
    value
        .map(|value| {
            anyhow::ensure!(value.len() == 10, "Claude {label} is not a calendar date");
            time::Date::parse(
                value,
                time::macros::format_description!("[year]-[month]-[day]"),
            )
            .map_err(|error| anyhow::anyhow!("Claude {label} is invalid: {error}"))?;
            Ok(value.to_owned())
        })
        .transpose()
}

fn plan_name(organization: &Organization) -> Result<String> {
    let tier = organization.rate_limit_tier.as_str();
    let analytics = organization.analytics_subscription_plan.as_deref();
    let known = if tier == "default_claude_max_20x" || analytics == Some("claude_max 20x") {
        Some("Max 20×")
    } else if tier == "default_claude_max_5x" || analytics == Some("claude_max 5x") {
        Some("Max 5×")
    } else if tier == "default_claude_pro" || analytics == Some("claude_pro") {
        Some("Pro")
    } else if analytics == Some("free") {
        Some("Free")
    } else {
        None
    };
    let name = known
        .map(str::to_owned)
        .or_else(|| organization.plan_display_name.clone())
        .or_else(|| organization.analytics_subscription_plan.clone())
        .context("Claude returned an unrecognized plan without a display name")?;
    validate_label(&name, "plan name")?;
    Ok(name)
}

fn validate_label(value: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control),
        "Claude returned an invalid {label}"
    );
    Ok(())
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}
