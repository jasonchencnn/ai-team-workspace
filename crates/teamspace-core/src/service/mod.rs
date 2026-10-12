//! 业务服务：每个函数在调用方提供的连接（通常是事务）上执行，校验规则并写活动记录。

pub(crate) mod event;
pub(crate) mod harness;
pub(crate) mod role;
pub(crate) mod task;
pub(crate) mod team;
