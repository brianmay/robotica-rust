//! Stream of data from a source.

use std::fmt::Formatter;
use std::ops::Deref;

use thiserror::Error;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::debug;
use tracing::error;

use crate::spawn;
use crate::PIPE_SIZE;

/// A stateless receiver
pub type StatelessReceiver<T> = Receiver<StatelessData<T>>;

/// A stateless weak receiver that won't hold the sender alive.
pub type StatelessWeakReceiver<T> = WeakReceiver<StatelessData<T>>;

/// A stateless sender
pub type StatelessSender<T> = Sender<StatelessData<T>>;

/// A subscription to a stateless receiver
pub type StatelessSubscription<T> = Subscription<StatelessData<T>>;

/// A stateful receiver
pub type StatefulReceiver<T> = Receiver<StatefulData<T>>;

/// A stateful weak receiver that won't hold the sender alive.
pub type StatefulWeakReceiver<T> = WeakReceiver<StatefulData<T>>;

/// A subscription to a stateful receiver
pub type StatefulSender<T> = Sender<StatefulData<T>>;

/// A subscription to a stateful receiver
pub type StatefulSubscription<T> = Subscription<StatefulData<T>>;

enum SendMessage<T> {
    Set(T),
}

/// Represents the type of the data, whether stateless or stateful
pub trait Data {
    /// The type of data that is sent to the receiver
    type Sent;

    /// The type of data that is received from the sender
    type Received: Clone;

    /// Create a new entity with the given name
    fn new_entity(name: impl Into<String>) -> (Sender<Self>, Receiver<Self>)
    where
        Self: Sized;

    /// Convert a received value to a sent value
    fn received_to_sent(data: Self::Received) -> Self::Sent;
}

/// A stateless data type
///
/// A stateless connection doesn't care about previous values, it just sends the latest value.
#[derive(Clone)]
pub struct StatelessData<T>(T);
impl<T: Clone + Send + 'static> Data for StatelessData<T> {
    type Sent = T;
    type Received = T;

    fn new_entity(name: impl Into<String>) -> (Sender<Self>, Receiver<Self>) {
        create_stateless_entity(name)
    }

    fn received_to_sent(data: Self::Received) -> Self::Sent {
        data
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for StatelessData<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("StatelessData").field(&self.0).finish()
    }
}

impl<T> Deref for StatelessData<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

type StreamableData<T> = (Option<T>, T);

/// A stateful data type
///
/// A stateful connection will try to keep track of the current state and sent it to new subscribers.
#[derive(Clone)]
pub struct StatefulData<T>(T);
impl<T: Clone + Send + Eq + 'static> Data for StatefulData<T> {
    type Sent = T;
    type Received = StreamableData<T>;

    fn new_entity(name: impl Into<String>) -> (Sender<Self>, Receiver<Self>) {
        create_stateful_entity(name)
    }

    fn received_to_sent(data: Self::Received) -> Self::Sent {
        data.1
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for StatefulData<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("StatefulData").field(&self.0).finish()
    }
}

#[allow(type_alias_bounds)]
type SubscribeMessage<T: Data> = (broadcast::Receiver<T::Received>, Option<T::Received>);

enum ReceiveMessage<T: Data> {
    Get(oneshot::Sender<Option<T::Received>>),
    Subscribe(oneshot::Sender<SubscribeMessage<T>>),
}

/// Send a value to an entity.
#[derive(Clone)]
pub struct Sender<T: Data> {
    #[allow(dead_code)]
    name: String,
    tx: mpsc::Sender<SendMessage<T::Sent>>,
}

impl<T: Data> Sender<T> {
    /// Send data to the entity.
    // pub async fn send(&self, data: T) {
    //     let msg = SendMessage::Set(data);
    //     if let Err(err) = self.tx.send(msg).await {
    //         error!("send failed: {}", err);
    //     }
    // }

    /// Send data to the entity or fail if buffer is full.
    pub fn try_send(&self, data: T::Sent) {
        let msg = SendMessage::Set(data);
        if let Err(err) = self.tx.try_send(msg) {
            error!("send failed: {}", err);
        }
    }

    /// Is the remote end of the channel closed?
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Completes when the entity is closed.
    pub async fn closed(&self)
    where
        T: Send,
        T::Sent: Send,
    {
        self.tx.closed().await;
    }
}

/// A `Receiver` that doesn't count as a reference to the entity.
pub struct WeakReceiver<T: Data> {
    name: String,
    tx: mpsc::WeakSender<ReceiveMessage<T>>,
}

impl<T: Data> WeakReceiver<T> {
    /// Try to convert to a `Receiver`.
    #[must_use]
    pub fn upgrade(&self) -> Option<Receiver<T>> {
        self.tx.upgrade().map(|tx| Receiver {
            name: self.name.clone(),
            tx,
        })
    }
}

/// Receive a value from an entity.
#[derive(Debug, Clone)]
pub struct Receiver<T: Data> {
    name: String,
    tx: mpsc::Sender<ReceiveMessage<T>>,
}

impl<T: Data + Send> Receiver<T>
where
    T::Received: Send,
{
    /// Subscribe to this entity.
    ///
    /// Returns an already closed subscription if the entity is closed.
    pub async fn subscribe(&self) -> Subscription<T> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Subscribe(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: subscribe/send failed: {}", self.name, err);
            return Subscription::null(self.tx.clone());
        };
        if let Ok((rx, initial)) = rx.await {
            Subscription {
                rx,
                _tx: self.tx.clone(),
                initial,
            }
        } else {
            error!("{}: subscribe/await failed", self.name);
            Subscription::null(self.tx.clone())
        }
    }

    /// Translate this receiver into a another type using a stateless receiver.
    #[must_use]
    fn translate_anything<U>(self) -> Receiver<U>
    where
        T: 'static,
        T::Sent: Send + 'static,
        T::Received: Send + 'static,
        U: Data + Send + 'static,
        U::Sent: TryFrom<T::Sent> + Clone + Send + 'static,
        <U::Sent as TryFrom<T::Sent>>::Error: Send + std::error::Error,
    {
        let name = format!("{} (translate_anything)", self.name);
        let (tx, rx) = U::new_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv_value() => {
                        let data = match data {
                            Ok(data) => T::received_to_sent(data),
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        match U::Sent::try_from(data) {
                            Ok(data) => {
                                tx.try_send(data);
                            }
                            Err(err) => {
                                error!("{name}: parse failed: {err}");
                            }
                        }
                    }

                    _ = tx.closed() => {
                        debug!("{name}: source closed");
                        break;
                    }
                }
            }
        });

        rx
    }

    /// Translate this receiver into a another type using a stateful receiver.
    // #[must_use]
    // pub fn translate_into_stateful<U>(self) -> Receiver<StatefulData<U>>
    // where
    //     T: Send + 'static,
    //     U: TryFrom<T> + Clone + Eq + Send + 'static,
    //     <U as TryFrom<T>>::Error: Send + std::error::Error,
    // {
    //     let name = format!("{} (translate_into_stateful)", self.name);
    //     let (tx, rx) = create_stateful_entity(&name);

    //     spawn(async move {
    //         let mut sub = self.subscribe().await;

    //         loop {
    //             select! {
    //                 data = sub.recv() => {
    //                     let data = match data {
    //                         Ok(data) => data,
    //                         Err(err) => {
    //                             debug!("{}: translate_into_stateful({}): recv failed, exiting", name, err);
    //                             break;
    //                         }
    //                     };

    //                     match U::try_from(data) {
    //                         Ok(data) => {
    //                             tx.try_send(data);
    //                         }
    //                         Err(err) => {
    //                             error!("translate_into_stateful({}): parse failed: {}", name, err);
    //                         }
    //                     }
    //                 }

    //                 _ = tx.closed() => {
    //                     debug!("translate_into_stateful({}): source closed", name);
    //                     break;
    //                 }
    //             }
    //         }
    //     });

    //     rx
    // }

    /// Map this receiver into a another type using a stateless receiver.
    // #[must_use]
    // pub fn map_into_stateless<U>(self, f: impl Fn(T) -> U + Send + 'static) -> Receiver<U>
    // where
    //     T: Send + 'static,
    //     U: Clone + Send + 'static,
    // {
    //     let name = format!("{} (map_into_stateless)", self.name);
    //     let (tx, rx) = create_stateless_entity(name);
    //     self.map(tx, f);
    //     rx
    // }

    /// Map this receiver into a another type using a stateful receiver.
    // #[must_use]
    // pub fn map_into_stateful<U>(
    //     self,
    //     f: impl Fn(T) -> U + Send + 'static,
    // ) -> Receiver<StatefulData<U>>
    // where
    //     T: Send + 'static,
    //     U: Clone + Eq + Send + 'static,
    // {
    //     let name = format!("{} (map_into_stateful)", self.name);
    //     let (tx, rx) = create_stateful_entity(name);
    //     self.map(tx, f);
    //     rx
    // }

    /// Map this receiver into a another type using a any type of receiver.
    fn map_into_anything<U>(
        self,
        f: impl Fn(T::Received) -> U::Sent + Send + 'static,
    ) -> Receiver<U>
    where
        T: 'static,
        T::Sent: Send + 'static,
        T::Received: Send + 'static,
        U: Data + Send + 'static,
        U::Sent: Send + 'static,
        U::Received: Send + 'static,
    {
        let name = format!("{} (map_into_anything)", self.name);
        let (tx, rx) = U::new_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv_value() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        tx.try_send(f(data));
                    }

                    _ = tx.closed() => {
                        debug!("{name}: dest closed");
                        break;
                    }
                }
            }
        });

        rx
    }

    // fn filter(self, f: impl Fn(&T) -> bool + Send + 'static) -> Receiver<T>
    // where
    //     T: Data,
    // {
    //     let name = format!("{} (map_into_stateless)", self.name);
    //     // let (tx, rx) = create_stateless_entity(name);
    //     // rx
    //     todo!();
    // }

    /// Map this receiver into a another type using a stateless receiver.
    // #[must_use]
    // pub fn filter_into_stateless(self, f: impl Fn(&T) -> bool + Send + 'static) -> Receiver<T>
    // where
    //     T: Send + 'static,
    // {
    //     let name = format!("{} (map_into_stateless)", self.name);
    //     let (tx, rx) = create_stateless_entity(name);
    //     self.filter(tx, f);
    //     rx
    // }

    /// Map this receiver into a another type using a stateful receiver.
    // #[must_use]
    // pub fn filter_into_stateful(
    //     self,
    //     f: impl Fn(&T) -> bool + Send + 'static,
    // ) -> Receiver<StatefulData<T>>
    // where
    //     T: Send + Eq + 'static,
    // {
    //     let name = format!("{} (map_into_stateful)", self.name);
    //     let (tx, rx) = create_stateful_entity(name);
    //     self.filter(tx, f);
    //     rx
    // }

    /// Filter this receiver based on function result
    #[must_use]
    pub fn filter(self, f: impl Fn(&T::Received) -> bool + Send + 'static) -> Receiver<T>
    where
        T: 'static,
        T::Received: Send + 'static,
        T::Sent: Send + 'static,
    {
        let name = format!("{} (filter_into_anything)", self.name);
        let (tx, rx) = T::new_entity(&name);

        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                select! {
                    data = sub.recv_value() => {
                        let data = match data {
                            Ok(data) => data,
                            Err(err) => {
                                debug!("{name}: recv failed, exiting: {err}");
                                break;
                            }
                        };

                        if f(&data) {
                            tx.try_send(T::received_to_sent(data));
                        }
                    }

                    _ = tx.closed() => {
                        debug!("{name}: dest closed");
                        break;
                    }
                }
            }
        });
        rx
    }

    /// Run a function for every value received.
    pub fn for_each_value<F>(self, f: F)
    where
        F: Fn(T::Received) + Send + 'static,
        T: 'static,
        T::Sent: Send + 'static,
        T::Received: Send + 'static,
    {
        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                while let Ok(data) = sub.recv_value().await {
                    f(data);
                }
            }
        });
    }

    /// Completes when the entity is closed.
    pub async fn closed(&self) {
        self.tx.closed().await;
    }

    /// Create a new `WeakReceiver` from this Receiver.
    #[must_use]
    pub fn downgrade(&self) -> WeakReceiver<T> {
        WeakReceiver {
            tx: self.tx.downgrade(),
            name: self.name.clone(),
        }
    }

    /// Run a function for every value received.
    pub fn for_each<F>(self, f: F)
    where
        F: Fn(T::Received) + Send + 'static,
        T: Send + 'static,
    {
        spawn(async move {
            let mut sub = self.subscribe().await;

            loop {
                while let Ok(data) = sub.recv_value().await {
                    f(data);
                }
            }
        });
    }
}

impl<T: Send + Clone + 'static> Receiver<StatelessData<T>> {
    //     /// Get the most recent value from the entity.
    //     pub async fn get(&self) -> Option<T> {
    //         self.get_value().await
    //     }

    //     pub fn for_each<F>(self, f: F)
    //     where
    //         F: Fn(T) + Send + 'static,
    //         T: Send + 'static,
    //     {
    //         self.for_each_value(f)
    //     }

    // pub fn new(name: impl Into<String>) -> (Sender<StatelessData<T>>, Receiver<StatelessData<T>>)
    // where
    //     T: 'static,
    // {
    //     create_stateless_entity(name)
    // }

    /// Subscribe to this entity and translate the data.
    // pub async fn subscribe_into<U>(&self) -> Subscription<StatelessData<U>>
    // where
    //     T: Send + 'static,
    //     U: TryFrom<T> + Clone + Send + 'static,
    //     <U as TryFrom<T>>::Error: Send + std::error::Error,
    // {
    //     let s = self.translate_into::<U>();
    //     s.subscribe().await
    // }

    /// Translate this receiver into a another stateless receiver using a stateless receiver.
    #[must_use]
    pub fn translate<U>(self) -> Receiver<StatelessData<U>>
    where
        T: Send + 'static,
        U: TryFrom<T> + Clone + Send + 'static,
        <U as TryFrom<T>>::Error: Send + std::error::Error,
    {
        self.translate_anything()
    }

    /// Translate this receiver into a stateful receiver using a stateless receiver.
    #[must_use]
    pub fn translate_stateful<U>(self) -> Receiver<StatefulData<U>>
    where
        T: Send + 'static,
        U: TryFrom<T> + Clone + Send + Eq + 'static,
        <U as TryFrom<T>>::Error: Send + std::error::Error,
    {
        self.translate_anything()
    }

    /// Translate this receiver into a another type using a stateless receiver.
    #[must_use]
    pub fn into_stateful(self) -> Receiver<StatefulData<T>>
    where
        T: Eq + 'static,
    {
        self.translate_anything()
    }

    /// Translate this receiver into a another type using a stateless receiver.
    #[must_use]
    pub fn map<U>(self, f: impl Fn(T) -> U + Send + 'static) -> Receiver<StatelessData<U>>
    where
        T: 'static,
        U: Clone + Send + 'static,
        // T::Sent: Send + 'static,
        // T::Received: Send + 'static,
        // U::Sent: Send + 'static,
        // U::Received: Send + 'static,
    {
        self.map_into_anything(f)
    }

    /// Translate this receiver into a another type using a stateful receiver.
    #[must_use]
    pub fn map_stateful<U>(self, f: impl Fn(T) -> U + Send + 'static) -> Receiver<StatefulData<U>>
    where
        T: 'static,
        U: Clone + Send + 'static + Eq,
        // T::Sent: Send + 'static,
        // T::Received: Send + 'static,
        // U::Sent: Send + 'static,
        // U::Received: Send + 'static,
    {
        self.map_into_anything(f)
    }

    // #[must_use]
    // pub fn filter(self, f: impl Fn(&T) -> bool + Send + 'static) -> Receiver<StatefulData<T>>
    // where
    //     T: Eq, // T::Received: Send + 'static,
    //            // T::Sent: Send + 'static,
    // {
    //     self.filter_into_anything(f);
    // }
}

// impl<T: Send + Clone + Eq> Receiver<StatelessData<T>> {
//     /// Translate this receiver into a another type using a stateful receiver.
//     #[must_use]
//     pub fn into_stateful(self) -> Receiver<StatefulData<T>> {
//         self.translate_into_anything()
//     }
// }

impl<T: Send + Clone + Eq + 'static> Receiver<StatefulData<T>> {
    // pub fn new(name: impl Into<String>) -> (Sender<StatefulData<T>>, Receiver<StatefulData<T>>) {
    //     create_stateful_entity(name)
    // }

    /// Retrieve the most recent value from the entity.
    ///
    /// Returns `None` if the entity is closed.
    pub async fn get_value(&self) -> Option<StreamableData<T>> {
        let (tx, rx) = oneshot::channel();
        let msg = ReceiveMessage::Get(tx);
        if let Err(err) = self.tx.send(msg).await {
            error!("{}: get/send failed: {}", self.name, err);
            return None;
        };
        (rx.await).map_or_else(
            |_| {
                error!("{}: get/await failed", self.name);
                None
            },
            |v| v,
        )
    }

    /// Get the most recent value from the entity.
    pub async fn get(&self) -> Option<T> {
        self.get_value().await.map(|(_prev, current)| current)
    }

    /// Subscribe to this entity and translate the data.
    // pub async fn subscribe_into<U>(&self) -> Subscription<StatefulData<U>>
    // where
    //     T: Send + 'static,
    //     U: TryFrom<T> + Clone + Send + Eq + 'static,
    //     <U as TryFrom<T>>::Error: Send + std::error::Error,
    // {
    //     let s = self.translate_into::<U>();
    //     s.subscribe().await
    // }

    /// Translate this receiver into a stateful receiver using a stateful receiver.
    #[must_use]
    pub fn translate<U>(self) -> Receiver<StatefulData<U>>
    where
        T: Send + 'static,
        U: TryFrom<T> + Clone + Send + Eq + 'static,
        <U as TryFrom<T>>::Error: Send + std::error::Error,
    {
        self.translate_anything()
    }

    /// Map this receiver into another type using a stateful receiver.
    #[must_use]
    pub fn map<U>(
        self,
        f: impl Fn(StreamableData<T>) -> U + Send + 'static,
    ) -> Receiver<StatefulData<U>>
    where
        T: 'static,
        U: Clone + Send + 'static + Eq,
        // T::Sent: Send + 'static,
        // T::Received: Send + 'static,
        // U::Sent: Send + 'static,
        // U::Received: Send + 'static,
    {
        self.map_into_anything(f)
    }
}

/// Something went wrong in Receiver.
#[derive(Error, Debug)]
pub enum RecvError {
    /// The Pipe was closed.
    #[error("The pipe was closed")]
    Closed,
}

/// A subscription to receive data from an entity.
pub struct Subscription<T: Data> {
    rx: broadcast::Receiver<T::Received>,
    // We need to keep this to ensure connection stays alive.
    _tx: mpsc::Sender<ReceiveMessage<T>>,
    initial: Option<T::Received>,
}

impl<T: Data> Subscription<T> {
    /// Create a null subscription that is already closed.
    fn null(tx: mpsc::Sender<ReceiveMessage<T>>) -> Self {
        let (_tx, rx) = broadcast::channel(0);
        Self {
            rx,
            _tx: tx,
            initial: None,
        }
    }
}

impl<T> Subscription<T>
where
    T: Data,
    T::Received: Send,
{
    /// Wait for the next value from the entity.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub async fn recv_value(&mut self) -> Result<T::Received, RecvError> {
        let initial = self.initial.take();
        if let Some(initial) = initial {
            return Ok(initial);
        }
        loop {
            match self.rx.recv().await {
                Ok(v) => return Ok(v),
                Err(err) => match err {
                    broadcast::error::RecvError::Closed => return Err(RecvError::Closed),
                    broadcast::error::RecvError::Lagged(_) => {
                        error!("recv failed: The pipe was lagged");
                    }
                },
            }
        }
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv_value(&mut self) -> Result<Option<T::Received>, RecvError> {
        let initial = self.initial.take();
        if let Some(initial) = initial {
            return Ok(Some(initial));
        }
        loop {
            match self.rx.try_recv() {
                Ok(v) => return Ok(Some(v)),
                Err(err) => match err {
                    broadcast::error::TryRecvError::Closed => {
                        return Err(RecvError::Closed);
                    }
                    broadcast::error::TryRecvError::Empty => return Ok(None),
                    broadcast::error::TryRecvError::Lagged(_) => {
                        error!("try_recv failed: The pipe was lagged");
                    }
                },
            }
        }
    }
}

impl<T: Send + Clone + 'static> Subscription<StatelessData<T>> {
    /// Wait for the next value from the entity.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        self.recv_value().await
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv(&mut self) -> Result<Option<T>, RecvError> {
        self.try_recv_value()
    }
}

impl<T: Send + Clone + Eq + 'static> Subscription<StatefulData<T>> {
    /// Wait for the next value from the entity.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        self.recv_value().await.map(|(_prev, current)| current)
    }

    /// Get the next value but don't wait for it. Returns `None` if there is no value.
    ///
    /// # Errors
    ///
    /// Returns `RecvError::Closed` if the entity is closed.
    pub fn try_recv(&mut self) -> Result<Option<T>, RecvError> {
        self.try_recv_value()
            .map(|opt| opt.map(|(_prev, current)| current))
    }
}

/// Create a stateless entity that sends every message.
#[must_use]
pub fn create_stateless_entity<T: Clone + Send + 'static>(
    name: impl Into<String>,
) -> (Sender<StatelessData<T>>, Receiver<StatelessData<T>>) {
    let (send_tx, mut send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, receive_rx) = mpsc::channel::<ReceiveMessage<StatelessData<T>>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<T>(PIPE_SIZE);

    drop(out_rx);

    let name = name.into();

    let sender: Sender<StatelessData<T>> = Sender {
        tx: send_tx,
        name: name.clone(),
    };
    let receiver: Receiver<StatelessData<T>> = Receiver {
        tx: receive_tx,
        name: name.clone(),
    };

    spawn(async move {
        let name = name;
        let mut receive_rx = Some(receive_rx);

        loop {
            select! {
                Some(msg) = send_rx.recv() => {
                    match msg {
                        SendMessage::Set(data) => {
                            if let Err(_err) = out_tx.send(data) {
                                // It is not an error if there are no subscribers.
                                // debug!("create_stateless_entity({name}): send to broadcast failed: {err} (not an error)");
                            }
                        }
                    }
                }
                Some(msg) = try_receive(&mut receive_rx) => {
                    match msg {
                        Some(ReceiveMessage::Get(tx)) => {
                            if tx.send(None).is_err() {
                                error!("create_stateless_entity{name}): get send failed");
                            };
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let rx = out_tx.subscribe();
                            if tx.send((rx, None)).is_err() {
                                error!("create_stateless_entity({name}): subscribe send failed");
                            };
                        }
                        None => {
                            debug!("create_stateless_entity({name}): command channel closed");
                            receive_rx = None;
                        }
                    }
                }
                else => {
                    debug!("create_stateless_entity({name}): all inputs closed");
                    break;
                }
            }

            if matches!((&receive_rx, out_tx.receiver_count()), (None, 0)) {
                debug!(
                    "create_stateless_entity({name}): receiver closed and all subscriptions closed"
                );
                break;
            }
        }
    });

    (sender, receiver)
}

/// Create a stateful entity that only produces messages when there is a change.
#[must_use]
pub fn create_stateful_entity<T: Clone + Eq + Send + 'static>(
    name: impl Into<String>,
) -> (Sender<StatefulData<T>>, Receiver<StatefulData<T>>) {
    let (send_tx, mut send_rx) = mpsc::channel::<SendMessage<T>>(PIPE_SIZE);
    let (receive_tx, receive_rx) = mpsc::channel::<ReceiveMessage<StatefulData<T>>>(PIPE_SIZE);
    let (out_tx, out_rx) = broadcast::channel::<StreamableData<T>>(PIPE_SIZE);

    drop(out_rx);

    let name = name.into();

    let sender = Sender {
        tx: send_tx,
        name: name.clone(),
    };
    let receiver = Receiver {
        tx: receive_tx,
        name: name.clone(),
    };

    spawn(async move {
        let name = name;

        let mut prev_data: Option<T> = None;
        let mut saved_data: Option<T> = None;
        let mut receive_rx = Some(receive_rx);

        loop {
            select! {
                Some(msg) = send_rx.recv() => {
                    match msg {
                        SendMessage::Set(data) => {
                            let changed = saved_data.as_ref().map_or(true, |saved_data| data != *saved_data);
                            if changed {
                                prev_data = saved_data.clone();
                                saved_data = Some(data.clone());
                                if let Err(_err) = out_tx.send((prev_data.clone(), data)) {
                                    // It is not an error if there are no subscribers.
                                    // debug!("create_stateful_entity({name}): send to broadcast failed: {err} (not an error)");
                                }
                            };
                        }
                    }
                }
                Some(msg) = try_receive(&mut receive_rx) => {
                    match msg {
                        Some(ReceiveMessage::Get(tx)) => {
                            let data = saved_data.clone().map(|data| (prev_data.clone(), data));
                            if tx.send(data).is_err() {
                                error!("create_stateful_entity({name}): get send failed");
                            };
                        }
                        Some(ReceiveMessage::Subscribe(tx)) => {
                            let data  = saved_data.clone().map(|data| (prev_data.clone(), data));
                            let rx = out_tx.subscribe();
                            if tx.send((rx, data)).is_err() {
                                error!("create_stateful_entity{name}): subscribe send failed");
                            };
                        }
                        None => {
                            debug!("create_stateful_entity({name}): command channel closed");
                            receive_rx = None;
                        }
                    }
                }
                else => {
                    debug!("create_stateful_entity({name}): all inputs closed");
                    break;
                }
            }

            if matches!((&receive_rx, out_tx.receiver_count()), (None, 0)) {
                debug!(
                    "create_stateful_entity({name}): receiver closed and all subscriptions closed"
                );
                break;
            }
        }
    });

    (sender, receiver)
}

async fn try_receive<T: Send>(rx: &mut Option<mpsc::Receiver<T>>) -> Option<Option<T>> {
    match rx {
        Some(rx) => Some(rx.recv().await),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[tokio::test]
    async fn test_stateless_entity() {
        let (tx, rx) = create_stateless_entity::<String>("test");
        let mut s = rx.subscribe().await;
        tx.try_send("hello".to_string());
        tx.try_send("goodbye".to_string());

        let current = s.recv().await.unwrap();
        assert_eq!("hello", current);

        let current = s.recv().await.unwrap();
        assert_eq!("goodbye", current);

        // let result = rx.get().await;
        // assert!(result.is_none());

        // let result = s.try_recv().unwrap();
        // assert!(result.is_none());

        // let result = rx.get().await;
        // assert!(result.is_none());

        drop(s);
        drop(rx);
        tx.closed().await;
    }

    #[tokio::test]
    async fn test_stateful_entity() {
        let (tx, rx) = create_stateful_entity::<String>("test");
        tx.try_send("hello".to_string());
        let mut s = rx.subscribe().await;
        tx.try_send("goodbye".to_string());

        let (prev, current) = s.recv_value().await.unwrap();
        assert_eq!(None, prev);
        assert_eq!("hello", current);

        let (prev, current) = s.recv_value().await.unwrap();
        assert_eq!("hello", prev.unwrap());
        assert_eq!("goodbye", current);

        let (prev, current) = rx.get_value().await.unwrap();
        assert_eq!("hello", prev.unwrap());
        assert_eq!("goodbye", current);

        let result = s.try_recv().unwrap();
        assert!(result.is_none());

        let (prev, current) = rx.get_value().await.unwrap();
        assert_eq!("hello", prev.unwrap());
        assert_eq!("goodbye", current);

        drop(s);
        drop(rx);
        tx.closed().await;
    }
}
