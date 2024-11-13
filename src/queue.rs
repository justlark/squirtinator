use core::fmt;

use esp_idf_svc::hal::{self, task::queue::Queue};

pub struct RendezvousQueue<T> {
    queue: Queue<T>,
}

impl<T> fmt::Debug for RendezvousQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RendezvousQueue").finish_non_exhaustive()
    }
}

impl<T> RendezvousQueue<T>
where
    T: Copy,
{
    pub fn new() -> Self {
        Self {
            queue: Queue::new(1),
        }
    }

    pub fn send(&self, value: T) {
        self.queue.send_back(value, hal::delay::BLOCK).ok();
    }

    pub fn try_send(&self, value: T) -> bool {
        self.queue.send_back(value, hal::delay::NON_BLOCK).is_ok()
    }

    pub fn recv(&self) -> T {
        if let Some((value, _)) = self.queue.recv_front(hal::delay::BLOCK) {
            value
        } else {
            unreachable!();
        }
    }

    pub fn try_recv(&self) -> Option<T> {
        if let Some((value, _)) = self.queue.recv_front(hal::delay::NON_BLOCK) {
            Some(value)
        } else {
            None
        }
    }

    pub fn try_peek(&self) -> Option<T> {
        self.queue.peek_front(hal::delay::NON_BLOCK)
    }
}
