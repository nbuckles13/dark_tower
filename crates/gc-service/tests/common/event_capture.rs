//! Capture `tracing` events for one target on the current thread (a
//! `#[sqlx::test]` is current_thread, so `set_default` sees every event the
//! test body emits). Fields are recorded as strings: `%x` and `?x` render
//! through Debug of their wrapper, `&str` fields verbatim; an `Option` field
//! that is `None` is ABSENT.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

/// One captured event: its level and fields (incl. `message`).
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    pub level: Level,
    pub fields: BTreeMap<String, String>,
}

impl CapturedEvent {
    /// A field's value, if recorded.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

type Store = Arc<Mutex<Vec<CapturedEvent>>>;

struct CaptureLayer {
    target: &'static str,
    store: Store,
}

struct FieldVisitor<'a>(&'a mut BTreeMap<String, String>);

impl Visit for FieldVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != self.target {
            return;
        }
        let mut fields = BTreeMap::new();
        event.record(&mut FieldVisitor(&mut fields));
        self.store.lock().unwrap().push(CapturedEvent {
            level: *event.metadata().level(),
            fields,
        });
    }
}

/// Events captured while the returned guard is alive.
pub struct EventCapture {
    store: Store,
    _guard: tracing::subscriber::DefaultGuard,
}

impl EventCapture {
    /// Start capturing events with exactly this `target`.
    pub fn install(target: &'static str) -> Self {
        let store: Store = Arc::default();
        let subscriber = tracing_subscriber::registry().with(CaptureLayer {
            target,
            store: Arc::clone(&store),
        });
        let guard = tracing::subscriber::set_default(subscriber);
        Self {
            store,
            _guard: guard,
        }
    }

    /// Drain everything captured so far.
    pub fn take(&self) -> Vec<CapturedEvent> {
        std::mem::take(&mut *self.store.lock().unwrap())
    }
}
