//! Keyset pages of the administration listings: each record once, in order, with a
//! cursor that names its own listing and a tenant that reads only its own records.
use super::*;
use crate::{
    AcquisitionCursor, AdminPageLimit, AuthorityDatasetKind, CalendarCursor, SourceCursor,
    TimeAcquisition, TimeAcquisitionId, TimeAcquisitionPhase, TimeAcquisitionStatus, TimeSource,
    TimeSourceId, TimeVersion,
};
use uuid::Uuid;

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
        let two = AdminPageLimit::new(2).unwrap();

        // Five sources page as 2, 2 and 1 in source ID order.
        let mut sources = Vec::new();
        for _ in 0..5 {
            sources.push(source(&catalog, &owner).await.source_id.clone());
        }
        let foreign_source = source(&catalog, &foreign).await;
        let mut walked = Vec::new();
        let mut sizes = Vec::new();
        let mut after: Option<SourceCursor> = None;
        loop {
            let page = catalog
                .sources_page(&owner, after.as_ref(), two)
                .await
                .unwrap();
            sizes.push(page.items.len());
            walked.extend(page.items.iter().map(|source| source.source_id.clone()));
            let Some(next) = page.next_cursor else { break };
            assert_eq!(Some(next.source_id()), walked.last());
            after = Some(returned(&next));
        }
        assert_eq!(sizes, [2, 2, 1]);
        sources.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        assert_eq!(walked, sources);
        let all = catalog
            .sources_page(&owner, None, AdminPageLimit::default())
            .await
            .unwrap();
        assert_eq!(all.items.len(), 5);
        assert!(all.next_cursor.is_none());
        // Another tenant's walk from the lowest position reads only its own source.
        let position = SourceCursor::new(
            &TimeSourceId::parse("time-source-00000000-0000-7000-8000-000000000000").unwrap(),
        );
        let other = catalog
            .sources_page(&foreign, Some(&position), AdminPageLimit::default())
            .await
            .unwrap();
        assert_eq!(other.items, [foreign_source]);

        // Four acquisitions page as 2 and 2, the most recently started first, and the
        // second page knows it is the last. Every principal of the tenant appears.
        let shared = source(&catalog, &owner).await;
        let mut acquisitions = Vec::new();
        for index in 0..4 {
            let by = if index % 2 == 0 { &owner } else { &peer };
            acquisitions.push(acquisition(&catalog, by, &shared).await.acquisition_id);
        }
        let first = catalog.acquisitions_page(&owner, None, two).await.unwrap();
        let next = first.next_cursor.as_ref().unwrap();
        assert_eq!(next.acquisition_id(), &first.items[1].acquisition_id);
        let second = catalog
            .acquisitions_page(&owner, Some(&returned(next)), two)
            .await
            .unwrap();
        assert!(second.next_cursor.is_none());
        let walked: Vec<_> = first
            .items
            .iter()
            .chain(&second.items)
            .map(|acquisition| acquisition.acquisition_id.clone())
            .collect();
        acquisitions.reverse();
        assert_eq!(walked, acquisitions);
        assert!(
            catalog
                .acquisitions_page(&foreign, None, AdminPageLimit::default())
                .await
                .unwrap()
                .items
                .is_empty()
        );

        // A cursor admits only the listing that issued it.
        let source_cursor = after.expect("the source walk continued past its first page");
        assert!(AcquisitionCursor::parse(source_cursor.as_str()).is_err());
        assert!(SourceCursor::parse(next.as_str()).is_err());
        let calendar = CalendarCursor::new(
            &CalendarId::parse("calendar-00000000-0000-7000-8000-000000000001").unwrap(),
            TimeVersion::FIRST,
        );
        assert!(SourceCursor::parse(calendar.as_str()).is_err());
        assert!(AcquisitionCursor::parse(calendar.as_str()).is_err());
        for limit in [0, 101] {
            assert!(AdminPageLimit::new(limit).is_err());
            assert!(serde_json::from_value::<AdminPageLimit>(limit.into()).is_err());
        }
        assert_eq!(AdminPageLimit::default(), AdminPageLimit::MAX);
        let request: crate::ListSourcesRequest =
            serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(request.limit.get(), 100);
        assert!(request.cursor.is_none());
    })
    .await
    .expect("administration page qualification exceeded 90 seconds");
}
