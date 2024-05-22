use std::collections::VecDeque;

const DEFAULT_FIFO_SIZE: usize = 16;

#[derive(Debug, Clone, Default)]
pub struct Fifo<T, const N: usize = DEFAULT_FIFO_SIZE> {
    queue: VecDeque<T>,
}

impl<T, const N: usize> Fifo<T, N> {
    pub fn new() -> Self {
        Fifo {
            queue: VecDeque::with_capacity(N)
        }
    }

    pub fn push(&mut self, item: T) -> Result<(), &'static str> {
        if self.queue.len() < N {
            self.queue.push_back(item);
            Ok(())
        } else {
            Err("fifo is full")
        }
    }

    pub fn peek(&mut self) -> Option<&T> {
        self.queue.front()
    }

    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.queue.len() == N
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}