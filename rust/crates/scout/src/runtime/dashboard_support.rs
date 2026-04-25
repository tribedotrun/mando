use crate::ScoutStatus;

pub(super) fn api_scout_status(status: ScoutStatus) -> api_types::ScoutItemStatus {
    match status {
        ScoutStatus::Pending => api_types::ScoutItemStatus::Pending,
        ScoutStatus::Fetched => api_types::ScoutItemStatus::Fetched,
        ScoutStatus::Processed => api_types::ScoutItemStatus::Processed,
        ScoutStatus::Saved => api_types::ScoutItemStatus::Saved,
        ScoutStatus::Archived => api_types::ScoutItemStatus::Archived,
        ScoutStatus::Error => api_types::ScoutItemStatus::Error,
    }
}

pub(super) fn api_scout_item(
    item: crate::ScoutItem,
    summary: Option<String>,
    telegraph_url: Option<String>,
) -> api_types::ScoutItem {
    let has_summary = summary.as_ref().map(|value| !value.is_empty());
    api_types::ScoutItem {
        id: item.id,
        rev: item.rev,
        url: item.url,
        title: item.title,
        status: api_scout_status(item.status),
        item_type: Some(item.item_type),
        summary,
        has_summary,
        relevance: item.relevance,
        quality: item.quality,
        date_added: Some(item.date_added),
        date_processed: item.date_processed,
        added_by: item.added_by,
        source_name: item.source_name,
        date_published: item.date_published,
        error_count: Some(item.error_count),
        research_run_id: item.research_run_id,
        telegraph_url,
    }
}

pub(super) fn bulk_result_status(
    success_count: u32,
    failure_count: usize,
) -> api_types::BulkResultStatus {
    if failure_count > 0 && success_count == 0 {
        api_types::BulkResultStatus::Error
    } else if failure_count > 0 {
        api_types::BulkResultStatus::Partial
    } else {
        api_types::BulkResultStatus::Ok
    }
}

pub(super) fn should_repair_article(status: ScoutStatus) -> bool {
    matches!(
        status,
        ScoutStatus::Processed | ScoutStatus::Saved | ScoutStatus::Archived
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_result_status_preserves_route_contract() {
        assert_eq!(bulk_result_status(2, 0), api_types::BulkResultStatus::Ok);
        assert_eq!(
            bulk_result_status(2, 1),
            api_types::BulkResultStatus::Partial
        );
        assert_eq!(bulk_result_status(0, 1), api_types::BulkResultStatus::Error);
    }
}
