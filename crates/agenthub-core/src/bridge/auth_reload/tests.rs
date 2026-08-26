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

#[test]
fn apply_reloaded_token_does_not_clobber_newer_revision() {
    let auth = ResolvedAuth::bearer("old");
    let observed = auth.revision();
    auth.replace_token("newer");
    assert!(!auth.apply_reloaded_token(observed, "from-reload"));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waiter_on_stale_reload_retries_without_clobbering_newer_credential() {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_cb = hits.clone();
    let phase = Arc::new(AtomicUsize::new(0));
    let started = Arc::new((Mutex::new(false), Condvar::new()));
    let started_cb = started.clone();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_cb = release.clone();
    let reload: crate::bridge::UpstreamAuthReload = Arc::new(move || {
        hits_cb.fetch_add(1, Ordering::SeqCst);
        if phase.fetch_add(1, Ordering::SeqCst) == 0 {
            return Some("rotated".into());
        }
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
        Some("from-reload".into())
    });
    let leader_member = member("acc-a", "old", reload);
    let waiter_member = PickedMember::new(
        leader_member.ticket_id.clone(),
        leader_member.source_kind.clone(),
        leader_member.source_id.clone(),
        leader_member.label.clone(),
        leader_member.auth.clone(),
        leader_member.reload.clone(),
        MemberHealth::Renewable,
    );
    let coordinator = AuthReloadCoordinator::new();
    assert_eq!(
        coordinator.reload_member(&leader_member).await,
        AuthReloadOutcome::Rotated
    );
    assert_eq!(leader_member.auth.token(), "rotated");

    let leader = tokio::spawn({
        let coordinator = coordinator.clone();
        let leader_member = leader_member.clone();
        async move { coordinator.reload_member(&leader_member).await }
    });
    {
        let (lock, cvar) = &*started;
        let mut ready = lock.lock().expect("lock");
        while !*ready {
            ready = cvar.wait(ready).expect("wait");
        }
    }
    leader_member.auth.replace_token("newer");
    let waiter = tokio::spawn({
        let coordinator = coordinator.clone();
        let waiter_member = waiter_member.clone();
        async move { coordinator.reload_member(&waiter_member).await }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    {
        let (lock, cvar) = &*release;
        *lock.lock().expect("lock") = true;
        cvar.notify_one();
    }
    let leader_out = leader.await.expect("join leader");
    let waiter_out = waiter.await.expect("join waiter");
    assert_eq!(leader_out, AuthReloadOutcome::Stale);
    assert!(
        waiter_out.should_retry(),
        "waiter must retry with the newer cell, not isolate: {waiter_out:?}"
    );
    assert_eq!(leader_member.auth.token(), "newer");
    assert_eq!(waiter_member.auth.token(), "newer");
    assert!(!coordinator.is_isolated("account:acc-a"));
    assert_eq!(hits.load(Ordering::SeqCst), 2);
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
