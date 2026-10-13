mod recovery;

use super::*;
use futures::StreamExt;
use std::time::Duration;
use veoveo_platform_store::{Instance, PlatformStore};
use veoveo_task_runtime::TaskStatus;
use veoveo_types::{
    AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

use crate::test_database::{TimeDb, slug};

fn authority() -> InvocationAuthority {
    let principal = PrincipalId::parse("integration-principal").unwrap();
    InvocationAuthority {
        work_context: WorkContextId::parse("integration-mission").unwrap(),
        tenant: TenantId::parse("integration-tenant").unwrap(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::parse("r1").unwrap(),
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        provenance: InvocationProvenance::Direct {
            initiator: principal,
        },
    }
}

fn owner() -> TaskOwner {
    TaskOwner {
        principal_key: "integration-principal".parse().unwrap(),
        principal_kind: veoveo_task_runtime::PrincipalKind::User,
        issuer: "https://issuer.integration.example".parse().unwrap(),
        subject: "integration-subject".parse().unwrap(),
        surface: "integration-profile".parse().unwrap(),
        tenant_key: Some("integration-tenant".parse().unwrap()),
        data_labels: BTreeSet::from(["internal".parse().unwrap()]),
        authority: authority(),
    }
}

/// A Time Task runtime with its own process instance, as one replica runs it.
async fn runtime(store: &PlatformStore) -> TaskRuntime {
    let instance = Instance::start(store, &slug()).await.unwrap();
    TaskRuntime::new(store.clone(), slug(), Arc::new(instance))
}

#[tokio::test]
async fn a_replica_runs_queued_time_tasks_to_completion() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = TimeDb::new().await;
        let creator = runtime(&db.a).await;
        let request = TimeTaskRequest::ValidateTimeline(
            crate::contract::ValidateTimelineRequestValue {
                points: vec![],
                constraints: vec![],
            }
            .build()
            .unwrap(),
        );
        let queued = creator
            .create(time_task(&creator, owner(), &request, BTreeSet::new()).unwrap())
            .await
            .unwrap()
            .snapshot;
        assert_eq!(queued.status, TaskStatus::Queued);
        // A replica that did not create the Task claims it from the queue.
        let replica = runtime(&db.b).await;
        let mut work = replica.work().await.unwrap();
        let claimed = work.next().await.unwrap().unwrap();
        assert_eq!(claimed.task_id, queued.task_id);
        replica.ensure_owner(&claimed).unwrap();
        assert!(creator.ensure_owner(&claimed).is_err());
    })
    .await
    .expect("Time task claim control exceeded 60 seconds");
}
