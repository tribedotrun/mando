use std::path::PathBuf;
use std::sync::Arc;

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
                                let mut value =
                                    serde_json::to_value(&entry).unwrap_or(serde_json::Value::Null);
                                if let serde_json::Value::Object(ref mut map) = value {
                                    if let Some(task_id) = entry.task_id {
                                        if let Some(title) = task_titles.get(&task_id) {
                                            map.insert(
                                                "task_title".into(),
                                                serde_json::Value::String(title.clone()),
                                            );
                                        }
                                    }
                                    if let Some(scout_id) = entry.scout_item_id {
                                        if let Some(title) = scout_titles.get(&scout_id) {
                                            map.insert(
                                                "scout_item_title".into(),
                                                serde_json::Value::String(title.clone()),
                                            );
                                        }
                                    }
                                    if let Some(credential_id) = entry.credential_id {
                                        if let Some(label) = cred_labels.get(&credential_id) {
                                            map.insert(
                                                "credential_label".into(),
                                                serde_json::Value::String(label.clone()),
                                            );
                                        }
                                    }
                                }
                                value
                            })
                            .collect();

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
            transcript_markdown: {
                let pool = query_pool.clone();
                Arc::new(move |session_id: String| {
                    let pool = pool.clone();
                    Box::pin(async move {
                        sessions::transcript_access::load_transcript_markdown(&pool, &session_id)
                            .await
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
        },
    ))
}
