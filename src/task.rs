use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub(crate) struct JoinHandle<T>(tokio::task::JoinHandle<T>);

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.0).poll(cx) {
            Poll::Ready(Ok(o)) => Poll::Ready(o),
            Poll::Ready(Err(e)) => {
                let Ok(panic) = e.try_into_panic() else {
                    unreachable!("we should only cancel the task when dropped, so we can't be polled again")
                };
                std::panic::resume_unwind(panic)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    JoinHandle(tokio::task::spawn(future))
}
