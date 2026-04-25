use std::path::PathBuf;
use std::sync::Arc;

fn wrap_session_result(
    result: global_claude::CcResult<serde_json::Value>,
) -> sessions::SessionAiResult {
    sessions::SessionAiResult {
        text: result.text,
        structured: result
            .structured
            .map(sessions::SessionStructuredOutput::from),
        session_id: result.session_id,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
        duration_api_ms: result.duration_api_ms,
        num_turns: result.num_turns,
        errors: result.errors,
        envelope: result.envelope,
        stream_path: result.stream_path,
        rate_limit: result.rate_limit,
        pid: result.pid,
        credential_id: result.credential_id,
    }
}

fn session_status(status: &str) -> anyhow::Result<api_types::SessionStatus> {
    match status {
        "running" => Ok(api_types::SessionStatus::Running),
        "stopped" => Ok(api_types::SessionStatus::Stopped),
        "failed" => Ok(api_types::SessionStatus::Failed),
        other => Err(anyhow::anyhow!("unknown session status: {other}")),
    }
}

fn session_category(group: sessions::CallerGroup) -> api_types::SessionCategory {
    match group {
        sessions::CallerGroup::Workers => api_types::SessionCategory::Workers,
        sessions::CallerGroup::Clarifier => api_types::SessionCategory::Clarifier,
        sessions::CallerGroup::CaptainReview => api_types::SessionCategory::CaptainReview,
        sessions::CallerGroup::CaptainOps => api_types::SessionCategory::CaptainOps,
        sessions::CallerGroup::Advisor => api_types::SessionCategory::Advisor,
        sessions::CallerGroup::Planning => api_types::SessionCategory::Planning,
        sessions::CallerGroup::TodoParser => api_types::SessionCategory::TodoParser,
        sessions::CallerGroup::Scout => api_types::SessionCategory::Scout,
        sessions::CallerGroup::Rebase => api_types::SessionCategory::Rebase,
    }
}

fn session_entry_from_row(
    entry: sessions::queries::SessionRow,
    task_titles: &std::collections::HashMap<i64, String>,
    scout_titles: &std::collections::HashMap<i64, String>,
    cred_labels: &std::collections::HashMap<i64, String>,
) -> anyhow::Result<api_types::SessionEntry> {
    let category = entry.group().map(session_category);
    let task_title = entry
        .task_id
        .and_then(|task_id| task_titles.get(&task_id).cloned());
    let scout_item_title = entry
        .scout_item_id
        .and_then(|scout_id| scout_titles.get(&scout_id).cloned());
    let credential_label = entry
        .credential_id
        .and_then(|credential_id| cred_labels.get(&credential_id).cloned());

    Ok(api_types::SessionEntry {
        session_id: entry.session_id,
        created_at: entry.created_at,
        cwd: entry.cwd,
        model: entry.model,
        caller: entry.caller,
        resumed: entry.resumed != 0,
        cost_usd: entry.cost_usd,
        duration_ms: entry.duration_ms,
        turn_count: Some(entry.turn_count),
        scout_item_id: entry.scout_item_id,
        task_id: entry.task_id.map(|id| id.to_string()),
        worker_name: entry.worker_name,
        resumed_at: entry.resumed_at,
        status: session_status(&entry.status)?,
        task_title,
        scout_item_title,
        github_repo: None,
        pr_number: None,
        worktree: None,
        branch: None,
        resume_cwd: None,
        category,
        credential_id: entry.credential_id,
        credential_label,
        error: entry.error,
        api_error_status: entry.api_error_status,
    })
}

pub fn build_sessions_runtime(
    state_dir: PathBuf,
    default_model: &str,
    pool: sqlx::SqlitePool,
) -> Arc<sessions::SessionsRuntime> {
    let query_pool = pool.clone();
    let manager = Arc::new(captain::CcSessionManager::new(
        state_dir,
        default_model,
        pool,
    ));

    Arc::new(sessions::SessionsRuntime::new(
        sessions::SessionsRuntimeOps {
            recover: {
                let manager = manager.clone();
                Arc::new(move || {
                    let stats = manager.recover();
                    sessions::RecoverStats {
                        recovered: stats.recovered,
                        corrupt: stats.corrupt,
                    }
                })
            },
            cleanup_expired: {
                let manager = manager.clone();
                Arc::new(move || manager.cleanup_expired())
            },
            has_session: {
                let manager = manager.clone();
                Arc::new(move |key| manager.has_session(key))
            },
            close: {
                let manager = manager.clone();
                Arc::new(move |key| manager.close(key))
            },
            close_async: {
                let manager = manager.clone();
                Arc::new(move |key: String| {
                    let manager = manager.clone();
                    Box::pin(async move {
                        manager.close_async(&key).await;
                    })
                })
            },
            start_with_item: {
                let manager = manager.clone();
                Arc::new(move |request: sessions::SessionStartRequest| {
                    let manager = manager.clone();
                    Box::pin(async move {
                        manager
                            .start_with_item(
                                &request.key,
                                &request.prompt,
                                &request.cwd,
                                request.model.as_deref(),
                                request.idle_ttl,
                                request.call_timeout,
                                request.task_id,
                                request.max_turns,
                            )
                            .await
                            .map(wrap_session_result)
                    })
                })
            },
            start_replacing: {
                let manager = manager.clone();
                Arc::new(move |request: sessions::SessionStartRequest| {
                    let manager = manager.clone();
                    Box::pin(async move {
                        manager
                            .start_replacing(
                                &request.key,
                                &request.prompt,
                                &request.cwd,
                                request.model.as_deref(),
                                request.idle_ttl,
                                request.call_timeout,
                            )
                            .await
                            .map(wrap_session_result)
                    })
                })
            },
            follow_up: {
                Arc::new(move |request: sessions::SessionFollowUpRequest| {
                    let manager = manager.clone();
                    Box::pin(async move {
                        manager
                            .follow_up(&request.key, &request.message, &request.cwd)
                            .await
                            .map(wrap_session_result)
                    })
                })
            },
            list_sessions: {
                let pool = query_pool.clone();
                Arc::new(move |query: sessions::SessionListQuery| {
                    let pool = pool.clone();
                    Box::pin(async move {
                        let (entries, total) = sessions::queries::list_sessions(
                            &pool,
                            query.page,
                            query.per_page,
                            query.category.as_deref(),
                            query.status.as_deref(),
                        )
                        .await?;

                        let categories = sessions::queries::category_counts(&pool)
                            .await?
                            .into_iter()
                            .map(|(name, count)| (name, count as u64))
                            .collect::<std::collections::BTreeMap<_, _>>();

                        let task_titles = captain::task_routing(&pool)
                            .await?
                            .into_iter()
                            .map(|task| (task.id, task.title))
                            .collect::<std::collections::HashMap<_, _>>();

                        let scout_ids: Vec<i64> = entries
                            .iter()
                            .filter_map(|entry| entry.scout_item_id)
                            .collect();
                        let scout_titles = scout::item_titles(&pool, &scout_ids)
                            .await
                            .unwrap_or_default();

                        let cred_ids: Vec<i64> = entries
                            .iter()
                            .filter_map(|entry| entry.credential_id)
                            .collect();
                        let cred_labels = settings::credentials::labels_by_ids(&pool, &cred_ids)
                            .await
                            .unwrap_or_default();

                        let sessions = entries
                            .into_iter()
                            .map(|entry| {
                                session_entry_from_row(
                                    entry,
                                    &task_titles,
                                    &scout_titles,
                                    &cred_labels,
                                )
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;

                        let total_pages = if total == 0 {
                            1
                        } else {
                            total.div_ceil(query.per_page)
                        };
                        let total_cost_usd = sessions::queries::total_session_cost(&pool).await?;

                        Ok(sessions::SessionListPage {
                            total,
                            page: query.page.min(total_pages),
                            per_page: query.per_page,
                            total_pages,
                            categories,
                            total_cost_usd: (total_cost_usd * 1000.0).round() / 1000.0,
                            sessions,
                        })
                    })
                })
            },
            session_cwd: {
                let pool = query_pool.clone();
                Arc::new(move |session_id: String| {
                    let pool = pool.clone();
                    Box::pin(
                        async move { sessions::queries::session_cwd(&pool, &session_id).await },
                    )
                })
            },
            session_jsonl_path: {
                let pool = query_pool.clone();
                Arc::new(move |session_id: String| {
                    let pool = pool.clone();
                    Box::pin(async move {
                        sessions::transcript_access::load_jsonl_path(&pool, &session_id).await
                    })
                })
            },
            session_messages: Arc::new(move |session_id: String, limit, offset| {
                Box::pin(async move {
                    sessions::transcript_access::load_messages(&session_id, limit, offset).await
                })
            }),
            session_tool_usage: Arc::new(move |session_id: String| {
                Box::pin(
                    async move { sessions::transcript_access::load_tool_usage(&session_id).await },
                )
            }),
            session_cost: Arc::new(move |session_id: String| {
                Box::pin(async move {
                    sessions::transcript_access::load_session_cost(&session_id).await
                })
            }),
            session_stream: Arc::new(move |session_id: String, types| {
                Box::pin(async move {
                    sessions::transcript_access::load_session_stream(&session_id, types).await
                })
            }),
            events_snapshot: {
                let pool = query_pool.clone();
                Arc::new(move |session_id: String| {
                    let pool = pool.clone();
                    Box::pin(async move {
                        sessions::transcript_access::load_events_snapshot(&pool, &session_id).await
                    })
                })
            },
        },
    ))
}
