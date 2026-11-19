use crate::local_db::LocalDbError;
use async_trait::async_trait;

#[cfg(target_family = "wasm")]
use js_sys::{Function, Object, Promise, Reflect};
#[cfg(target_family = "wasm")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_family = "wasm")]
use wasm_bindgen_utils::prelude::*;
#[cfg(target_family = "wasm")]
use web_sys::window;

#[cfg(any(target_family = "wasm", test))]
const LOCK_NAME_PREFIX: &str = "local-db-sync-engine";

/// Guard that keeps platform-specific leadership state alive for the duration of a run.
pub struct LeadershipGuard {
    #[cfg(target_family = "wasm")]
    release_fn: Option<Function>,
}

impl LeadershipGuard {
    /// Creates a guard that does not perform any release action when dropped.
    pub fn new_noop() -> Self {
        Self {
            #[cfg(target_family = "wasm")]
            release_fn: None,
        }
    }

    #[cfg(target_family = "wasm")]
    fn with_release(release_fn: Function) -> Self {
        Self {
            release_fn: Some(release_fn),
        }
    }
}

impl Drop for LeadershipGuard {
    fn drop(&mut self) {
        #[cfg(target_family = "wasm")]
        if let Some(release_fn) = self.release_fn.take() {
            let _ = release_fn.call0(&JsValue::UNDEFINED);
        }
    }
}

#[async_trait(?Send)]
pub trait Leadership {
    /// Attempts to establish leadership for the current runner invocation.
    ///
    /// Returns `None` when another leader is already active (browser Web Locks only).
    async fn acquire(&self) -> Result<Option<LeadershipGuard>, LocalDbError>;
}

#[derive(Clone, Debug)]
pub struct DefaultLeadership {
    #[cfg_attr(not(any(target_family = "wasm", test)), allow(dead_code))]
    network_key: Option<String>,
}

impl DefaultLeadership {
    pub fn new() -> Self {
        Self { network_key: None }
    }

    pub fn with_network_key(network_key: String) -> Self {
        Self {
            network_key: Some(network_key),
        }
    }

    #[cfg(any(target_family = "wasm", test))]
    fn lock_name(&self) -> String {
        match &self.network_key {
            Some(key) => format!("{}-{}", LOCK_NAME_PREFIX, key),
            None => LOCK_NAME_PREFIX.to_string(),
        }
    }
}

impl Default for DefaultLeadership {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Leadership for DefaultLeadership {
    async fn acquire(&self) -> Result<Option<LeadershipGuard>, LocalDbError> {
        #[cfg(target_family = "wasm")]
        {
            let lock_name = self.lock_name();
            match attempt_web_lock(&lock_name).await {
                Ok(Some(guard)) => Ok(Some(guard)),
                Ok(None) => Ok(None),
                Err(_) => Ok(Some(LeadershipGuard::new_noop())),
            }
        }

        #[cfg(not(target_family = "wasm"))]
        {
            Ok(Some(LeadershipGuard::new_noop()))
        }
    }
}

/// Convenience helper for callers that do not yet inject a leadership strategy.
pub async fn acquire() -> Result<Option<LeadershipGuard>, LocalDbError> {
    DefaultLeadership::new().acquire().await
}

/// A separate, short-lived lock covers every network's initial provisioning.
/// Unlike per-network leadership, never pretend ownership when Web Locks fail.
#[cfg(target_family = "wasm")]
pub(crate) async fn acquire_bootstrap() -> Result<Option<LeadershipGuard>, LocalDbError> {
    acquire_bootstrap_lock("local-db-bootstrap").await
}

/// Serialize browser dump preflight and import, including per-network fallback.
#[cfg(target_family = "wasm")]
pub(crate) async fn acquire_import() -> Result<Option<LeadershipGuard>, LocalDbError> {
    // Non-browser Wasm callers have no cross-tab ownership to coordinate.
    if window().is_none() {
        return Ok(Some(LeadershipGuard::new_noop()));
    }
    acquire_bootstrap_lock("local-db-sql-dump-import").await
}

/// Protect initial seed writes from existing per-network leaders in other tabs.
#[cfg(target_family = "wasm")]
pub(crate) async fn acquire_network_bootstrap(
    network_key: &str,
) -> Result<Option<LeadershipGuard>, LocalDbError> {
    let name = DefaultLeadership::with_network_key(network_key.to_owned()).lock_name();
    acquire_bootstrap_lock(&name).await
}

#[cfg(target_family = "wasm")]
async fn acquire_bootstrap_lock(lock_name: &str) -> Result<Option<LeadershipGuard>, LocalDbError> {
    let window = window().ok_or_else(|| {
        LocalDbError::CustomError("Browser bootstrap ownership is unavailable".to_string())
    })?;
    let locks = Reflect::get(window.navigator().as_ref(), &JsValue::from_str("locks"))
        .map_err(|_| LocalDbError::CustomError("Cannot access bootstrap Web Locks".to_string()))?;
    if locks.is_null() || locks.is_undefined() {
        return Err(LocalDbError::CustomError(
            "Coordinated bootstrap requires Web Locks".to_string(),
        ));
    }
    // Keep the callback alive if startup is stopped while the browser is still
    // scheduling the lock request. A closed receiver drops any granted guard,
    // releasing ownership instead of stranding it after cancellation.
    let lock_name = lock_name.to_owned();
    let (sender, receiver) = futures::channel::oneshot::channel();
    wasm_bindgen_futures::spawn_local(async move {
        let result = attempt_web_lock(&lock_name).await;
        let _ = sender.send(result);
    });
    receiver
        .await
        .map_err(|_| LocalDbError::CustomError("Bootstrap ownership task stopped".to_string()))?
        .map_err(|_| LocalDbError::CustomError("Cannot acquire bootstrap Web Lock".to_string()))
}

#[cfg(target_family = "wasm")]
async fn attempt_web_lock(lock_name: &str) -> Result<Option<LeadershipGuard>, JsValue> {
    use std::cell::RefCell;
    use std::rc::Rc;

    let window = match window() {
        Some(window) => window,
        None => {
            return Ok(Some(LeadershipGuard::new_noop()));
        }
    };
    let navigator = window.navigator();
    let locks_value = Reflect::get(navigator.as_ref(), &JsValue::from_str("locks"))?;
    if locks_value.is_undefined() || locks_value.is_null() {
        return Ok(Some(LeadershipGuard::new_noop()));
    }

    let request_fn =
        Reflect::get(&locks_value, &JsValue::from_str("request"))?.dyn_into::<Function>()?;

    let options = Object::new();
    Reflect::set(
        &options,
        &JsValue::from_str("mode"),
        &JsValue::from_str("exclusive"),
    )?;
    Reflect::set(
        &options,
        &JsValue::from_str("ifAvailable"),
        &JsValue::from_bool(true),
    )?;

    let acquired_resolver = Rc::new(RefCell::new(None::<Function>));
    let release_resolver = Rc::new(RefCell::new(None::<Function>));

    let acquired_promise = {
        let acquired_resolver = Rc::clone(&acquired_resolver);
        Promise::new(&mut move |resolve, _| {
            acquired_resolver.borrow_mut().replace(resolve.clone());
        })
    };
    let acquired_future = JsFuture::from(acquired_promise);

    let callback_acquired = Rc::clone(&acquired_resolver);
    let callback_release = Rc::clone(&release_resolver);
    let callback = Closure::wrap(Box::new(move |lock: JsValue| -> JsValue {
        if lock.is_undefined() || lock.is_null() {
            if let Some(resolve) = callback_acquired.borrow_mut().take() {
                let _ = resolve.call1(&JsValue::UNDEFINED, &JsValue::from_bool(false));
            }
            JsValue::UNDEFINED
        } else {
            if let Some(resolve) = callback_acquired.borrow_mut().take() {
                let _ = resolve.call1(&JsValue::UNDEFINED, &JsValue::from_bool(true));
            }
            let release_resolver = Rc::clone(&callback_release);
            Promise::new(&mut move |resolve, _| {
                release_resolver.borrow_mut().replace(resolve.clone());
            })
            .into()
        }
    }) as Box<dyn FnMut(JsValue) -> JsValue>);

    let request_result = request_fn.call3(
        &locks_value,
        &JsValue::from_str(lock_name),
        &options.into(),
        callback.as_ref().unchecked_ref(),
    );

    if request_result.is_err() {
        // Drop the callback before propagating the error.
        drop(callback);
        return Err(request_result
            .err()
            .unwrap_or_else(|| JsValue::from_str("Web Locks request failed")));
    }

    // request() can reject asynchronously without ever calling our callback.
    // Observe that rejection so the coordinator can fall back rather than wait
    // indefinitely for the acquired resolver.
    let request_future = JsFuture::from(request_result?.dyn_into::<Promise>()?);
    let acquired_value = match futures::future::select(acquired_future, request_future).await {
        futures::future::Either::Left((result, _)) => result?,
        futures::future::Either::Right((result, _)) => {
            result?;
            JsValue::FALSE
        }
    };
    drop(callback);

    if !acquired_value.as_bool().unwrap_or(false) {
        return Ok(None);
    }

    let release_fn = match release_resolver.borrow_mut().take() {
        Some(function) => function,
        None => {
            return Ok(Some(LeadershipGuard::new_noop()));
        }
    };

    Ok(Some(LeadershipGuard::with_release(release_fn)))
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::executor::block_on;

    #[test]
    fn acquire_returns_guard_in_native_env() {
        let guard = block_on(DefaultLeadership::new().acquire()).expect("acquire succeeds");
        assert!(
            guard.is_some(),
            "guard should be present when Web Locks are unavailable"
        );
    }

    struct DeterministicLeadership {
        grant: bool,
    }

    #[async_trait(?Send)]
    impl Leadership for DeterministicLeadership {
        async fn acquire(&self) -> Result<Option<LeadershipGuard>, LocalDbError> {
            if self.grant {
                Ok(Some(LeadershipGuard::new_noop()))
            } else {
                Ok(None)
            }
        }
    }

    #[test]
    fn stub_leadership_can_simulate_denied_lock() {
        let leadership = DeterministicLeadership { grant: false };
        let guard = block_on(leadership.acquire()).expect("acquire succeeds");
        assert!(
            guard.is_none(),
            "stub leadership should be able to simulate missing guard"
        );
    }

    #[test]
    fn stub_leadership_can_simulate_granted_lock() {
        let leadership = DeterministicLeadership { grant: true };
        let guard = block_on(leadership.acquire()).expect("acquire succeeds");
        assert!(
            guard.is_some(),
            "stub leadership should be able to simulate acquiring leadership"
        );
    }

    #[test]
    fn with_network_key_creates_leadership_with_key() {
        let leadership = DefaultLeadership::with_network_key("mainnet".to_string());
        assert_eq!(leadership.network_key, Some("mainnet".to_string()));
    }

    #[test]
    fn new_creates_leadership_without_key() {
        let leadership = DefaultLeadership::new();
        assert_eq!(leadership.network_key, None);
    }

    #[test]
    fn lock_name_without_network_key_uses_prefix_only() {
        let leadership = DefaultLeadership::new();
        assert_eq!(leadership.lock_name(), "local-db-sync-engine");
    }

    #[test]
    fn lock_name_with_network_key_includes_network() {
        let leadership = DefaultLeadership::with_network_key("mainnet".to_string());
        assert_eq!(leadership.lock_name(), "local-db-sync-engine-mainnet");
    }

    #[test]
    fn different_networks_produce_different_lock_names() {
        let leadership_a = DefaultLeadership::with_network_key("network-a".to_string());
        let leadership_b = DefaultLeadership::with_network_key("network-b".to_string());

        assert_eq!(leadership_a.lock_name(), "local-db-sync-engine-network-a");
        assert_eq!(leadership_b.lock_name(), "local-db-sync-engine-network-b");
        assert_ne!(leadership_a.lock_name(), leadership_b.lock_name());
    }
}

#[cfg(all(test, target_family = "wasm", feature = "browser-tests"))]
mod wasm_tests {
    use super::*;
    use js_sys::{Function, Object, Promise, Reflect};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use wasm_bindgen::prelude::Closure;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use wasm_bindgen_test::*;
    use web_sys::window;

    wasm_bindgen_test_configure!(run_in_browser);

    struct LockStub {
        hook: LockHook,
        _request_closure: Closure<dyn FnMut(JsValue, JsValue, JsValue) -> JsValue>,
        release_promise: Rc<RefCell<Option<Promise>>>,
        release_invoked: Rc<Cell<bool>>,
        _release_then: Rc<RefCell<Option<Closure<dyn FnMut(JsValue)>>>>,
    }

    enum LockHook {
        Replace {
            navigator: web_sys::Navigator,
        },
        Patch {
            locks: Object,
            original_request: JsValue,
        },
    }

    impl LockStub {
        fn install(grant_lock: bool) -> Result<Self, JsValue> {
            let window = window().ok_or_else(|| JsValue::from_str("window unavailable"))?;
            let navigator = window.navigator();

            let callback_result = if grant_lock {
                JsValue::from(Object::new())
            } else {
                JsValue::UNDEFINED
            };

            let release_promise = Rc::new(RefCell::new(None::<Promise>));
            let release_invoked = Rc::new(Cell::new(false));
            let release_then = Rc::new(RefCell::new(None::<Closure<dyn FnMut(JsValue)>>));

            let promise_cell = Rc::clone(&release_promise);
            let flag_cell = Rc::clone(&release_invoked);
            let then_cell = Rc::clone(&release_then);

            let request_closure: Closure<dyn FnMut(JsValue, JsValue, JsValue) -> JsValue> =
                Closure::wrap(Box::new(
                    move |_name: JsValue, _options: JsValue, cb: JsValue| -> JsValue {
                        let callback: Function = cb.unchecked_into();
                        let promise_js = match callback.call1(&JsValue::UNDEFINED, &callback_result)
                        {
                            Ok(value) => value,
                            Err(_) => return Promise::resolve(&JsValue::UNDEFINED).into(),
                        };

                        if let Some(promise) = promise_js.dyn_ref::<Promise>() {
                            let promise = promise.clone();
                            promise_cell.borrow_mut().replace(promise.clone());

                            let flag = Rc::clone(&flag_cell);
                            let then_closure: Closure<dyn FnMut(JsValue)> =
                                Closure::wrap(Box::new(move |_value: JsValue| {
                                    flag.set(true);
                                })
                                    as Box<dyn FnMut(JsValue)>);
                            let _ = promise.then(&then_closure);
                            *then_cell.borrow_mut() = Some(then_closure);

                            promise.into()
                        } else {
                            Promise::resolve(&JsValue::UNDEFINED).into()
                        }
                    },
                )
                    as Box<dyn FnMut(JsValue, JsValue, JsValue) -> JsValue>);

            let locks_value = Reflect::get(navigator.as_ref(), &JsValue::from_str("locks"))?;
            let hook = if locks_value.is_undefined() || locks_value.is_null() {
                let locks = Object::new();
                Reflect::set(
                    &locks,
                    &JsValue::from_str("request"),
                    request_closure.as_ref().unchecked_ref(),
                )?;
                Reflect::set(
                    navigator.as_ref(),
                    &JsValue::from_str("locks"),
                    &locks.into(),
                )?;

                LockHook::Replace { navigator }
            } else {
                let locks: Object = locks_value.dyn_into()?;
                let original_request = Reflect::get(&locks, &JsValue::from_str("request"))?;
                Reflect::set(
                    &locks,
                    &JsValue::from_str("request"),
                    request_closure.as_ref().unchecked_ref(),
                )?;

                LockHook::Patch {
                    locks,
                    original_request,
                }
            };

            Ok(Self {
                hook,
                _request_closure: request_closure,
                release_promise,
                release_invoked,
                _release_then: release_then,
            })
        }

        async fn await_release(&self) {
            if let Some(promise) = self.release_promise.borrow().clone() {
                let _ = JsFuture::from(promise).await;
            }
        }

        fn release_called(&self) -> bool {
            self.release_invoked.get()
        }
    }

    impl Drop for LockStub {
        fn drop(&mut self) {
            if let Some(_) = window() {
                match &self.hook {
                    LockHook::Replace { navigator } => {
                        let _ = Reflect::delete_property(
                            navigator.as_ref(),
                            &JsValue::from_str("locks"),
                        );
                    }
                    LockHook::Patch {
                        locks,
                        original_request,
                    } => {
                        if original_request.is_undefined() || original_request.is_null() {
                            let _ = Reflect::delete_property(locks, &JsValue::from_str("request"));
                        } else {
                            let _ = Reflect::set(
                                locks,
                                &JsValue::from_str("request"),
                                original_request,
                            );
                        }
                    }
                }
            }
        }
    }

    #[wasm_bindgen_test(async)]
    async fn acquire_returns_guard_when_lock_granted() {
        let _stub = LockStub::install(true).expect("stub install");
        let guard = DefaultLeadership::new()
            .acquire()
            .await
            .expect("acquire ok");
        assert!(guard.is_some(), "expected guard when lock granted");
    }

    #[wasm_bindgen_test(async)]
    async fn acquire_returns_none_when_lock_denied() {
        let _stub = LockStub::install(false).expect("stub install");
        let guard = DefaultLeadership::new()
            .acquire()
            .await
            .expect("acquire ok");
        assert!(guard.is_none(), "expected None when lock denied");
    }

    #[wasm_bindgen_test(async)]
    async fn guard_drop_invokes_release_callback() {
        let stub = LockStub::install(true).expect("stub install");
        {
            let guard_opt = DefaultLeadership::new()
                .acquire()
                .await
                .expect("acquire ok");
            let guard = guard_opt.expect("expected guard when lock granted");
            drop(guard);
        }
        stub.await_release().await;
        assert!(
            stub.release_called(),
            "expected release callback to run when guard dropped"
        );
    }

    #[wasm_bindgen_test(async)]
    async fn cohort_and_network_runner_cannot_own_the_same_network() {
        let network = DefaultLeadership::with_network_key("cohort-review-test".to_string());
        let runner_guard = network.acquire().await.unwrap().unwrap();
        assert!(acquire_network_bootstrap("cohort-review-test")
            .await
            .unwrap()
            .is_none());
        drop(runner_guard);
        let cohort_guard = acquire_network_bootstrap("cohort-review-test")
            .await
            .unwrap()
            .unwrap();
        assert!(network.acquire().await.unwrap().is_none());
        drop(cohort_guard);
        // Resolving the release promise schedules Web Locks cleanup; an
        // immediate ifAvailable request may still observe the old ownership.
        let mut reacquired = None;
        for _ in 0..20 {
            gloo_timers::future::TimeoutFuture::new(1).await;
            reacquired = network.acquire().await.unwrap();
            if reacquired.is_some() {
                break;
            }
        }
        assert!(reacquired.is_some());
    }

    #[wasm_bindgen_test(async)]
    async fn bootstrap_ownership_excludes_other_startups_and_releases() {
        // Scheduler tests may still be unwinding production bootstrap tasks.
        // Exercise the same ownership implementation with an isolated name.
        const NAME: &str = "test-bootstrap-ownership";
        let guard = acquire_bootstrap_lock(NAME).await.unwrap().unwrap();
        assert!(acquire_bootstrap_lock(NAME).await.unwrap().is_none());
        // The bootstrap lock must not collide with normal network leadership.
        let network_guard = DefaultLeadership::with_network_key("test".to_string())
            .acquire()
            .await
            .unwrap()
            .unwrap();
        drop(network_guard);
        drop(guard);
        assert!(acquire_bootstrap_lock(NAME).await.unwrap().is_some());
    }

    #[wasm_bindgen_test(async)]
    async fn cancelled_bootstrap_request_does_not_strand_ownership() {
        const NAME: &str = "test-cancelled-bootstrap";
        let mut pending = Box::pin(acquire_bootstrap_lock(NAME));
        assert!(futures::poll!(&mut pending).is_pending());
        drop(pending);
        gloo_timers::future::TimeoutFuture::new(10).await;
        assert!(acquire_bootstrap_lock(NAME).await.unwrap().is_some());
    }

    #[wasm_bindgen_test(async)]
    async fn rejected_bootstrap_request_returns_an_error() {
        let _stub = LockStub::install(true).unwrap();
        let locks = Reflect::get(window().unwrap().navigator().as_ref(), &"locks".into()).unwrap();
        Reflect::set(
            &locks,
            &"request".into(),
            &Function::new_no_args("return Promise.reject(new Error('unavailable'));"),
        )
        .unwrap();
        assert!(acquire_bootstrap().await.is_err());
    }
}
