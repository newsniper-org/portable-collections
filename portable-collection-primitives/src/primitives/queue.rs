use super::container::Container;

pub trait Push<T> : Container {
    fn push(&mut self, item: T);
}

pub trait TryPush<T> : Push<T> {
    fn try_push(&mut self, item: T) -> bool;
}

pub trait Pop<T> : Container {
    fn pop(&mut self) -> Option<T>;
    fn last(&self) -> Option<&T>;
}

pub trait Pull<T> : Container {
    fn pull(&mut self) -> Option<T>;
    fn first(&self) -> Option<&T>;
}