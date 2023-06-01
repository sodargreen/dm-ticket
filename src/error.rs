use thiserror::Error;

// Api返回的错误信息
#[derive(Error, Debug)]
pub enum DmApiError {
    #[error("B-00203-200-034::您选购的商品信息已过期，请重新查询")]
    ProductEpired,

    #[error("RGV587_ERROR::SM::哎哟喂,被挤爆啦,请稍后重试")]
    SystemBusy,
}
