use std::fmt::Debug;

#[derive(Debug)]
pub enum NoError {}

pub trait MaybeResult<T> {
    type Error: Debug;

    fn as_result(self) -> Result<T, Self::Error>;
}

impl<T> MaybeResult<T> for T {
    type Error = NoError;

    fn as_result(self) -> Result<T, Self::Error> {
        Ok(self)
    }
}

impl<T> MaybeResult<T> for Option<T> {
    type Error = ();

    fn as_result(self) -> Result<T, Self::Error> {
        self.ok_or(())
    }
}

impl<T, E> MaybeResult<T> for Result<T, E>
where
    E: Debug,
{
    type Error = E;

    fn as_result(self) -> Self {
        self
    }
}

#[derive(Debug)]
pub enum AppError<E1, E2> {
    WindowError(E1),
    DataError(E2),
}
