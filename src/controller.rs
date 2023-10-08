use std::{sync::Arc, future::Future};

use tokio::{sync::RwLock, task::JoinHandle};
use tokio_context::task::TaskController;

pub struct Controller {
    inner: Arc<RwLock<Option<TaskController>>>
}

impl Controller {
    pub fn new() -> Self {
        return Self {
            inner: Arc::new(RwLock::new(Some(TaskController::new())))
        }
    }

    /// Cancelled controller will be refreshed. It can be reused again since this point
    pub async fn cancel(&mut self) {
        let mut target = self.inner.write().await;
        let ctrl = target.replace(TaskController::new());
        match ctrl {
            Some(inner) => {
                inner.cancel();
            },
            None => {
                panic!("controller has no inner controller!");
            }
        }
    }

    pub async fn spawn<T>(&mut self, future:T) -> JoinHandle<Option<T::Output>>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let mut controller_mut = self.inner.write().await;
        match controller_mut.as_mut() {
            Some(ctx) => {
                let result = ctx.spawn(future);
                return result;
            },
            None => {
                panic!("can't spawn when context is not initialized!")
            }
        }
    }
}

impl Clone for Controller {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}