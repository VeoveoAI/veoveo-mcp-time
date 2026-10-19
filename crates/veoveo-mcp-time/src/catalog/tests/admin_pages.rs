//! Keyset pages of the administration listings: each record once, in order, with a
//! cursor that names its own listing and a tenant that reads only its own records.
use super::*;
use crate::{
    Acquisitions, AuthorityDatasetKind, Sources, TimeAcquisition, TimeAcquisitionId,
    TimeAcquisitionPhase, TimeAcquisitionStatus, TimeSource, TimeSourceId, TimeVersion,
};
use uuid::Uuid;
use veoveo_hub_sdk::definition::{CollectionRequest, List, NoFilters};
use veoveo_types::{CursorToken, PageSize};

async fn source(catalog: &TimeCatalog, scope: &TimeAccessContext) -> TimeSource {
    catalog
        .create_source(
            scope,
            crate::NewTimeSourceValue {
                source_id: TimeSourceId::parse(format!("time-source-{}", Uuid::now_v7())).unwrap(),
                name: "fixture".into(),
                dataset_kind: AuthorityDatasetKind::Tzdb,
                url: "https://example.test/tzdb".into(),
                expected_content_type: "application/gzip".into(),
                enabled: true,
                record_version: crate::SourceCreationVersion,
            }
            .build()
            .unwrap(),
        )
        .await
        .unwrap()
}

async fn acquisition(
    catalog: &TimeCatalog,
    scope: &TimeAccessContext,
    source: &TimeSource,
) -> TimeAcquisition {
    let now = Utc::now();
    catalog
        .create_acquisition(
            scope,
            TimeAcquisition {
                acquisition_id: TimeAcquisitionId::parse(format!(
                    "time-acquisition-{}",
                    Uuid::now_v7()
                ))
                .unwrap(),
                source_id: source.source_id.clone(),
                expected_source_digest: None,
                status: TimeAcquisitionStatus::Queued,
                phase: TimeAcquisitionPhase::Queued,
                staged_release_id: None,
                message: "queued".into(),
                created_at: now,
                updated_at: now,
                record_version: TimeVersion::FIRST,
            },
            Uuid::now_v7().to_string(),
        )
        .await
        .unwrap()
}

/// A cursor survives the JSON a caller returns it in.
fn returned<C: serde::Serialize + serde::de::DeserializeOwned>(cursor: &C) -> C {
    serde_json::from_value(serde_json::to_value(cursor).unwrap()).unwrap()
}

#[tokio::test]
async fn source_and_acquisition_pages_walk_every_record_once() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database::TimeDb::new().await;
        let catalog = TimeCatalog::new(db.a.pool().clone());
        let owner = scope(&db.a, "admin-pages", "owner").await;
        let peer = scope(&db.a, "admin-pages", "peer").await;
        let foreign = scope(&db.a, "admin-pages-other", "owner").await;
        let two = PageSize::new(2).unwrap();

        // Five sources page as 2, 2 and 1 in source ID order.
        let mut sources = Vec::new();
        for _ in 0..5 {
            sources.push(source(&catalog, &owner).await.source_id.clone());
        }
        let foreign_source = source(&catalog, &foreign).await;
        let mut walked = Vec::new();
        let mut sizes = Vec::new();
        let mut after: Option<CursorToken<Sources>> = None;
        loop {
            let page = catalog
                .sources_page(
                    &owner,
                    &match after.as_ref() {
                        Some(cursor) => CollectionRequest::<Sources>::new(NoFilters {}, two)
                            .continuing(cursor.clone()),
                        None => CollectionRequest::<Sources>::new(NoFilters {}, two),
                    },
                )
                .await
                .unwrap();
            sizes.push(page.items().len());
            walked.extend(page.items().iter().map(|source| source.source_id.clone()));
            let Some(next) = page.next_cursor() else {
                break;
            };
            after = Some(returned(next));
        }
        assert_eq!(sizes, [2, 2, 1]);
        sources.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        assert_eq!(walked, sources);
        let all = catalog
            .sources_page(&owner, &selection::<Sources>())
            .await
            .unwrap();
        assert_eq!(all.items().len(), 5);
        assert!(all.next_cursor().is_none());
        // A serialized continuation is refused under a foreign tenant.
        let source_cursor = after.as_ref().unwrap();
        let foreign_selection =
            CollectionRequest::<Sources>::new(NoFilters {}, two).continuing(source_cursor.clone());
        assert!(
            catalog
                .sources_page(&foreign, &foreign_selection)
                .await
                .is_err()
        );
        let other = catalog
            .sources_page(&foreign, &selection::<Sources>())
            .await
            .unwrap();
        assert_eq!(other.items(), &[foreign_source]);

        // Four acquisitions page as 2 and 2, the most recently started first, and the
        // second page knows it is the last. Every principal of the tenant appears.
        let shared = source(&catalog, &owner).await;
        let mut acquisitions = Vec::new();
        for index in 0..4 {
            let by = if index % 2 == 0 { &owner } else { &peer };
            acquisitions.push(acquisition(&catalog, by, &shared).await.acquisition_id);
        }
        let first = catalog
            .acquisitions_page(
                &owner,
                &CollectionRequest::<Acquisitions>::new(NoFilters {}, two),
            )
            .await
            .unwrap();
        let next = first.next_cursor().unwrap();
        let second = catalog
            .acquisitions_page(
                &owner,
                &CollectionRequest::<Acquisitions>::new(NoFilters {}, two)
                    .continuing(returned(next)),
            )
            .await
            .unwrap();
        assert!(second.next_cursor().is_none());
        let walked: Vec<_> = first
            .items()
            .iter()
            .chain(second.items())
            .map(|acquisition| acquisition.acquisition_id.clone())
            .collect();
        acquisitions.reverse();
        assert_eq!(walked, acquisitions);
        assert!(
            catalog
                .acquisitions_page(&foreign, &selection::<Acquisitions>())
                .await
                .unwrap()
                .items()
                .is_empty()
        );

        // Returning the same wire token through a different family's DTO does not admit it.
        let wrong: CursorToken<Acquisitions> =
            serde_json::from_value(serde_json::to_value(source_cursor).unwrap()).unwrap();
        assert!(
            catalog
                .acquisitions_page(
                    &owner,
                    &CollectionRequest::<Acquisitions>::new(NoFilters {}, two).continuing(wrong)
                )
                .await
                .is_err()
        );
        let changed = CollectionRequest::<Sources>::new(NoFilters {}, PageSize::new(1).unwrap())
            .continuing(source_cursor.clone());
        assert!(catalog.sources_page(&owner, &changed).await.is_err());
        let request: List<crate::ListSources> =
            serde_json::from_value(serde_json::json!({"filters":{}})).unwrap();
        assert_eq!(request.selection().limit().get(), 100);
    })
    .await
    .expect("administration page qualification exceeded 90 seconds");
}
