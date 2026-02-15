use super::*;

#[derive(Debug, Clone)]
pub enum HandlerValue {
    ReadyData(ReadyData),
    Message(Message),
    DeletedMessage(DeletedMessage),
    Reaction(Reaction),
    GuildCreate(GuildCreate),
    Interaction(Box<Interaction>),
    RoleDelete(RoleDelete),
    RoleEvent(RoleEvent),
    Reconnect(Reconnect),
    Member(Member),
    MemberLeave(MemberLeave),
}

impl From<ReadyData> for HandlerValue { fn from(value: ReadyData) -> Self { HandlerValue::ReadyData(value) } }
impl From<Message> for HandlerValue { fn from(value: Message) -> Self { HandlerValue::Message(value) } }
impl From<DeletedMessage> for HandlerValue { fn from(value: DeletedMessage) -> Self { HandlerValue::DeletedMessage(value) } }
impl From<Reaction> for HandlerValue { fn from(value: Reaction) -> Self { HandlerValue::Reaction(value) } }
impl From<GuildCreate> for HandlerValue { fn from(value: GuildCreate) -> Self { HandlerValue::GuildCreate(value) } }
impl From<Interaction> for HandlerValue { fn from(value: Interaction) -> Self { HandlerValue::Interaction(Box::new(value)) } }
impl From<RoleDelete> for HandlerValue { fn from(value: RoleDelete) -> Self { HandlerValue::RoleDelete(value) } }
impl From<RoleEvent> for HandlerValue { fn from(value: RoleEvent) -> Self { HandlerValue::RoleEvent(value) } }
impl From<Reconnect> for HandlerValue { fn from(value: Reconnect) -> Self { HandlerValue::Reconnect(value) } }
impl From<Member> for HandlerValue { fn from(value: Member) -> Self { HandlerValue::Member(value) } }
impl From<MemberLeave> for HandlerValue { fn from(value: MemberLeave) -> Self { HandlerValue::MemberLeave(value) } }

#[derive(Debug, Clone)]
pub struct EventHandler {
    pub event: Event,
    pub handler_fn: EventHandlerFn,
}

pub type EventHandlerFn = fn(
    HandlerValue,
) -> std::pin::Pin<
    Box<dyn futures_util::Future<Output = HandlerResult> + Send + 'static>,
>;

impl EventHandler {
    pub async fn call(&self, data: HandlerValue) -> HandlerResult {
        let fut = ((self.handler_fn)(data));
        let boxed_fut: std::pin::Pin<
            Box<dyn std::future::Future<Output = HandlerResult> + Send + 'static>,
        > = Box::pin(fut);
        boxed_fut.await
    }
}
