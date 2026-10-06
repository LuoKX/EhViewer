//! Application-owned, thread-scoped dictionary and metadata allocation limits.
//! Outside a scope, codec behavior is unchanged; bounds never leak to other readers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

std::thread_local! {
    #[cfg_attr(
        target_os = "android",
        allow(
            clippy::missing_const_for_thread_local,
            reason = "Pinned Android macro expansion warns despite the explicit const initializer"
        )
    )]
    static LIMITS: Cell<(usize, u64)> = const { Cell::new((usize::MAX, u64::MAX)) };
    #[cfg_attr(target_os = "android", allow(clippy::missing_const_for_thread_local, reason = "Const Android TLS fallback expansion"))]
    static BUDGET: RefCell<Option<Rc<AllocationState>>> = const { RefCell::new(None) };
}

/// Run decoding with dictionary and XZ-index bounds, restoring prior limits on unwind.
/// Limits are thread-local: independent archive readers and 7z operations are unaffected.
pub fn with_decoder_limits<T>(
    dictionary_bytes: usize,
    index_records: u64,
    operation: impl FnOnce() -> T,
) -> T {
    struct Restore((usize, u64));
    impl Drop for Restore {
        fn drop(&mut self) {
            LIMITS.with(|limits| limits.set(self.0));
        }
    }
    let previous = LIMITS.with(|limits| {
        let old = limits.get();
        limits.set((old.0.min(dictionary_bytes), old.1.min(index_records)));
        old
    });
    let _restore = Restore(previous);
    operation()
}

/// Reject a dictionary or `PPMd` model before allocating its declared capacity.
///
/// # Errors
/// Returns `OutOfMemory` when the active thread limit would be exceeded.
pub fn check_dictionary(size: usize) -> std::io::Result<()> {
    if LIMITS.with(|limits| size > limits.get().0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::OutOfMemory,
            "archive decoder dictionary exceeds memory limit",
        ));
    }
    Ok(())
}

/// Reject oversized XZ indexes before reserving their entry table.
///
/// # Errors
/// Returns `OutOfMemory` when the active thread record limit would be exceeded.
pub fn check_index(records: u64) -> std::io::Result<()> {
    if LIMITS.with(|limits| records > limits.get().1) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::OutOfMemory,
            "archive XZ index exceeds entry limit",
        ));
    }
    Ok(())
}

struct AllocationState {
    limit: usize,
    used: Cell<usize>,
}

/// Shared heap budget for simultaneously live nested decoders on one thread.
#[derive(Clone)]
pub struct AllocationBudget(Rc<AllocationState>);

impl AllocationBudget {
    /// Create a live-allocation budget; no memory is reserved until decoding.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self(Rc::new(AllocationState {
            limit,
            used: Cell::new(0),
        }))
    }

    /// Run with this budget. An existing outer budget remains authoritative.
    /// Tickets retain their budget after this scope returns and release on drop.
    pub fn run<T>(&self, operation: impl FnOnce() -> T) -> T {
        struct Restore(Option<Rc<AllocationState>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                BUDGET.with(|budget| *budget.borrow_mut() = self.0.take());
            }
        }
        let previous = BUDGET.with(|budget| {
            let previous = budget.borrow().clone();
            if previous.is_none() {
                *budget.borrow_mut() = Some(self.0.clone());
            }
            previous
        });
        let _restore = Restore(previous);
        operation()
    }
}

/// Owns one decoder's memory credit until its dictionary/model is destroyed.
pub struct DictionaryReservation {
    state: Option<Rc<AllocationState>>,
    size: usize,
}

impl Drop for DictionaryReservation {
    fn drop(&mut self) {
        if let Some(state) = &self.state {
            state.used.set(state.used.get() - self.size);
        }
    }
}

/// Check per-decoder bounds and reserve shared credit before allocating.
///
/// # Errors
/// Returns `OutOfMemory` if per-decoder or aggregate memory would exceed its budget.
pub fn reserve_dictionary(size: usize) -> std::io::Result<DictionaryReservation> {
    check_dictionary(size)?;
    let state = BUDGET.with(|budget| budget.borrow().clone());
    if let Some(state) = &state {
        let used = state
            .used
            .get()
            .checked_add(size)
            .filter(|used| *used <= state.limit)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::OutOfMemory,
                    "nested archive decoder memory limit exceeded",
                )
            })?;
        state.used.set(used);
    }
    Ok(DictionaryReservation { state, size })
}

/// Charge cumulative codec allocation work to the active budget before allocation.
/// Caller uses a fresh budget per operation and drops decoded temporary state before
/// returning. Conservative cumulative accounting never understates live heap.
///
/// # Errors
/// Returns `OutOfMemory` for size overflow, per-allocation or cumulative budget failure.
pub fn charge_allocation(size: usize) -> std::io::Result<()> {
    check_dictionary(size)?;
    BUDGET.with(|budget| {
        if let Some(state) = budget.borrow().as_ref() {
            let used = state
                .used
                .get()
                .checked_add(size)
                .filter(|used| *used <= state.limit)
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::OutOfMemory,
                        "codec allocation budget exceeded",
                    )
                })?;
            state.used.set(used);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;
