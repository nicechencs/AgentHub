use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::bridge::account::{MemberHealth, PickedMember};
use crate::bridge::runtime::ResolvedAuth;

use super::{AuthReloadCoordinator, AuthReloadOutcome};

fn member(id: &str, token: &str, reload: crate::bridge::UpstreamAuthReload) -> PickedMember {
    PickedMember::new(
        format!("account:{id}"),
        "account",
        id,
        id,
        ResolvedAuth::bearer(token),
        Some(reload),
        MemberHealth::Renewable,
    )
}

#[test]
fn stale_revision_does_not_clobber_newer_token() {
    let auth = ResolvedAuth::bearer("old");
    let observed = auth.revision();
    auth.replace_token("newer");
    assert!(!auth.replace_token_at_revision(observed, "from-reload"));
    assert_eq!(auth.token(), "newer");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_reloads_invoke_callback_once() {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_cb = hits.clone();
    let started = Arc::new((Mutex::new(false), Condvar::new()));
    let started_cb = started.clone();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_cb = release.clone();
    let reload: crate::bridge::UpstreamAuthReload = Arc::new(move || {
        hits_cb.fetch_add(1, Ordering::SeqCst);
        {
            let (lock, cvar) = &*started_cb;
            *lock.lock().expect("lock") = true;
            cvar.notify_one();
        }
        let (lock, cvar) = &*release_cb;
        let mut go = lock.lock().expect("lock");
        while !*go {
            go = cvar.wait(go).expect("wait");
        }
        Some("rotated".into())
    });
    let a = member("acc-a", "old", reload);
    let b = PickedMember::new(
        a.ticket_id.clone(),
        a.source_kind.clone(),
        a.source_id.clone(),
        a.label.clone(),
        a.auth.clone(),
        a.reload.clone(),
        MemberHealth::Renewable,
    );
    let coordinator = AuthReloadCoordinator::new();
    let first = tokio::spawn({
        let coordinator = coordinator.clone();
        let a = a.clone();
        async move { coordinator.reload_member(&a).await }
    });
    {
        let (lock, cvar) = &*started;
        let mut ready = lock.lock().expect("lock");
        while !*ready {
            ready = cvar.wait(ready).expect("wait");
        }
    }
    let second = tokio::spawn({
        let coordinator = coordinator.clone();
        let b = b.clone();
        async move { coordinator.reload_member(&b).await }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    {
        let (lock, cvar) = &*release;
        *lock.lock().expect("lock") = true;
        cvar.notify_one();
    }
    let one = first.await.expect("join first");
    let two = second.await.expect("join second");
    assert!(one.should_retry());
    assert!(two.should_retry());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(a.auth.token(), "rotated");
    assert_eq!(b.auth.token(), "rotated");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn in_flight_reload_cannot_overwrite_newer_credential() {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_cb = hits.clone();
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let started = pair.clone();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_cb = release.clone();
    let reload: crate::bridge::UpstreamAuthReload = Arc::new(move || {
        hits_cb.fetch_add(1, Ordering::SeqCst);
        {
            let (lock, cvar) = &*started;
            *lock.lock().expect("lock") = true;
            cvar.notify_one();
        }
        let (lock, cvar) = &*release_cb;
        let mut go = lock.lock().expect("lock");
        while !*go {
            go = cvar.wait(go).expect("wait");
        }
        Some("from-reload".into())
    });
    let picked = member("acc-a", "old", reload);
    let coordinator = AuthReloadCoordinator::new();
    let auth = picked.auth.clone();
    let reload_task = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.reload_member(&picked).await }
    });
    {
        let (lock, cvar) = &*pair;
        let mut ready = lock.lock().expect("lock");
        while !*ready {
            ready = cvar.wait(ready).expect("wait");
        }
    }
    auth.replace_token("newer");
    {
        let (lock, cvar) = &*release;
        *lock.lock().expect("lock") = true;
        cvar.notify_one();
    }
    let outcome = reload_task.await.expect("join");
    assert_eq!(outcome, AuthReloadOutcome::Stale);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(auth.token(), "newer");
}

#[test]
fn isolate_is_shared_by_fingerprint_not_by_request() {
    let coordinator = AuthReloadCoordinator::new();
    coordinator.isolate("account:acc-a");
    assert!(coordinator.is_isolated("account:acc-a"));
    assert!(!coordinator.is_isolated("account:acc-b"));
    coordinator.clear_isolated("account:acc-a");
    assert!(!coordinator.is_isolated("account:acc-a"));
}
