use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::{
    clients::dm::DmClient,
    config::Account,
    error::DmApiError,
    models::{
        order::{OrderForm, OrderInfo, OrderParams, SubmitOrderParams},
        perform::{PerformForm, PerformInfo, PerformParams},
        ticket::{TicketInfo, TicketInfoForm, TicketInfoParams},
        user::{GetUserInfoForm, GetUserInfoParams, UserInfoData},
        DmRes,
    },
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Local};
use log::{debug, error, info, warn};
use serde_json::json;
use tokio::signal;

const SUCCESS_FLAG: &str = "SUCCESS::调用成功";

pub struct DmTicket {
    pub client: DmClient,
    pub account: Account,
}

impl DmTicket {
    pub async fn new(account: Account) -> Result<Self> {
        let cookie = account
            .cookie
            .clone()
            .replace(' ', "")
            .split(';')
            .filter(|e| !e.starts_with("_m_h5_tk"))
            .collect::<Vec<&str>>()
            .join(";");

        let client = DmClient::new(cookie).await?;

        Ok(Self { client, account })
    }

    // 获取用户信息
    pub async fn get_user_info(&self) -> Result<UserInfoData> {
        let url = "https://mtop.damai.cn/h5/mtop.damai.wireless.user.session.transform/1.0/";
        let params = GetUserInfoParams::build()?;
        let form = GetUserInfoForm::build()?;
        let res = self.client.request(url, params, form).await?;
        if res.ret.contains(&SUCCESS_FLAG.to_string()) {
            let user_info_data = serde_json::from_value(res.data)?;
            Ok(user_info_data)
        } else {
            Err(anyhow!("{}", res.ret[0]))
        }
    }

    // 获取门票信息
    pub async fn get_ticket_info(&self, ticket_id: String) -> Result<TicketInfo> {
        let url = "https://mtop.damai.cn/h5/mtop.alibaba.damai.detail.getdetail/1.2";

        let params = TicketInfoParams::build()?;

        let data = TicketInfoForm::build(ticket_id)?;

        let res = self.client.request(url, params, data).await?;

        match res.ret.contains(&SUCCESS_FLAG.to_string()) {
            true => {
                debug!("获取门票信息成功, {:?}", res);

                let ticket_info: TicketInfo =
                    serde_json::from_str(res.data["result"].clone().as_str().unwrap())?;
                Ok(ticket_info)
            }
            false => {
                error!("获取门票信息失败, 结果:{:?}", res.ret);
                Err(anyhow!("获取门票信息失败..."))
            }
        }
    }

    // 生成订单
    pub async fn build_order(
        &self,
        item_id: &String,
        sku_id: &String,
        buy_num: usize,
    ) -> Result<OrderInfo> {
        let start = Instant::now();

        let url = "https://mtop.damai.cn/h5/mtop.trade.order.build.h5/4.0/?";

        let params = OrderParams::build()?;

        let data = OrderForm::build(item_id, sku_id, buy_num)?;

        let res = self.client.request(url, params, data).await?;

        debug!("生成订单结果:{:?}, 花费时间:{:?}", res, start.elapsed());

        match res.ret.contains(&SUCCESS_FLAG.to_string()) {
            true => {
                let order_info: OrderInfo = serde_json::from_value(res.data)?;
                Ok(order_info)
            }
            false => Err(anyhow!("{:?}", res.ret)),
        }
    }

    // 提交订单
    pub async fn submit_order(&self, order_info: OrderInfo) -> Result<DmRes> {
        let start = Instant::now();

        let url = "https://mtop.damai.cn/h5/mtop.trade.order.create.h5/4.0/";

        // 添加提交订单需要的数据
        let mut order_data = json!({});

        for key in order_info.linkage.input.iter() {
            if key.starts_with("dmViewer_") {
                let mut item = order_info.data[key].clone();
                let mut num = self.account.ticket.num;

                let viewer_list = item["fields"]["viewerList"].clone();

                // 需选择实名观演人
                if viewer_list.is_array() && !viewer_list.as_array().unwrap().is_empty() {
                    // 实名观演人比购票数量少
                    if viewer_list.as_array().unwrap().len() < num {
                        warn!("实名观演人小于实际购票数量, 请先添加实名观演人!");
                        num = viewer_list.as_array().unwrap().len();
                    }
                    if self.account.ticket.real_names.is_empty() {
                        info!("未配置实名观演人, 默认选择前{}位观演人...", self.account.ticket.num);
                        for i in 0..num {
                            item["fields"]["viewerList"][i]["isUsed"] = true.into();
                        }
                    }else{
                        for i in 0..item["fields"]["viewerList"].as_array().unwrap_or(&Vec::new()).len() {
                            let idx = i + 1;
                            if self.account.ticket.real_names.contains(&idx) {
                                item["fields"]["viewerList"][i]["isUsed"] = true.into();
                            }
                        }
                    }
                    
                }
                order_data[key] = item;
            } else {
                order_data[key] = order_info.data[key].clone();
            }
        }

        // 添加confirmOrder_1
        let confirm_order_key = &order_info.hierarchy.root;
        order_data[confirm_order_key] = order_info.data[confirm_order_key].clone();

        // 添加order_xxxxx
        let keys_list = order_info.hierarchy.structure[confirm_order_key].clone();
        for k in keys_list.as_array().unwrap() {
            let s = k.as_str().unwrap();
            if s.starts_with("order_") {
                order_data[s] = order_info.data[s].clone();
            }
        }

        let order_hierarchy = json!({
            "structure": order_info.hierarchy.structure
        });

        let order_linkage = json!({
            "common": {
                "compress": order_info.linkage.common.compress,
                "submitParams": order_info.linkage.common.submit_params,
                "validateParams": order_info.linkage.common.validate_params,
            },
            "signature": order_info.linkage.signature,
        });

        let submit_order_params = SubmitOrderParams::build(order_info.global.secret_value)?;

        let feature = json!({
            "subChannel": "damai@damaih5_h5",
            "returnUrl": "https://m.damai.cn/damai/pay-success/index.html?spm=a2o71.orderconfirm.bottom.dconfirm&sqm=dianying.h5.unknown.value",
            "serviceVersion": "2.0.0",
            "dataTags": "sqm:dianying.h5.unknown.value"
        });
        let params = json!({
            "data": serde_json::to_string(&order_data)?,
            "hierarchy": serde_json::to_string(&order_hierarchy)?,
            "linkage": serde_json::to_string(&order_linkage)?,
        });
        let sumbit_order_data = json!({
            "params": serde_json::to_string(&params)?,
            "feature": serde_json::to_string(&feature)?,
        });

        let res = self
            .client
            .request(url, submit_order_params, sumbit_order_data)
            .await?;

        debug!("提交订单结果:{:?}, 花费时间:{:?}", res, start.elapsed());
        Ok(res)
    }

    // 获取场次/票档信息
    pub async fn get_perform_info(
        &self,
        ticket_id: &String,
        perform_id: &String,
    ) -> Result<PerformInfo> {
        let start = Instant::now();

        let url = "https://mtop.damai.cn/h5/mtop.alibaba.detail.subpage.getdetail/2.0/";

        let params = PerformParams::build()?;

        let data = PerformForm::build(ticket_id, perform_id)?;

        let res = self.client.request(url, params, data).await?;

        debug!("获取演出票档信息:{:?}, 花费时间:{:?}", res, start.elapsed());

        let perform_info: PerformInfo = serde_json::from_str(res.data["result"].as_str().unwrap())?;

        Ok(perform_info)
    }

    // 购买流程
    pub async fn buy(&self, item_id: &String, sku_id: &String, buy_num: usize) -> Result<bool> {
        let start = Instant::now();

        let order_info = match self.build_order(item_id, sku_id, buy_num).await {
            Ok(data) => {
                info!("成功生成订单...");
                data
            }
            Err(e) => {
                info!("生成订单失败, {}", e.to_string());
                if e.to_string().contains(&DmApiError::SystemBusy.to_string()) {
                    return Err(anyhow!(DmApiError::SystemBusy));
                }
                return Err(e);
            }
        };

        let wait_for_submit_time = self.account.wait_for_submit_time;
        tokio::time::sleep(Duration::from_millis(wait_for_submit_time)).await;

        let res = self.submit_order(order_info).await?;

        match res.ret.contains(&SUCCESS_FLAG.to_string()) {
            true => {
                info!(
                    "提交订单成功, 请尽快前往手机APP付款,  此次抢购花费时间:{:?}",
                    start.elapsed()
                );
                Ok(true)
            }
            false => {
                info!(
                    "提交订单失败, 原因:{}, 此次抢购花费时间:{:?}",
                    res.ret[0],
                    start.elapsed()
                );
                Ok(false)
            }
        }
    }

    // 毫秒转时分秒
    pub fn ms_to_hms(&self, ms: i64) -> (u64, u64, f64) {
        let sec = ms as f64 / 1000.0;
        let hour = (sec / 3600.0) as u64;
        let rem = sec % 3600.0;
        let min = (rem / 60.0) as u64;
        let sec = rem % 60.0;
        (hour, min, sec)
    }

    // 尝试多次购买
    pub async fn multiple_buy_attempts(
        &self,
        item_id: &String,
        sku_id: &String,
        buy_num: Option<usize>,
    ) -> Result<bool> {
        let buy_num = match buy_num {
            Some(num) => num,
            None => self.account.ticket.num,
        };
        let retry_times = self.account.retry_times;
        let retry_interval = self.account.retry_interval;
        for _ in 0..retry_times {
            match self.buy(item_id, sku_id, buy_num).await {
                Ok(res) => {
                    if res {
                        return Ok(true);
                    }
                }
                Err(e) => {
                    // B-00203-200-034::您选购的商品信息已过期，请重新查询
                    if e.to_string()
                        .contains(&DmApiError::ProductEpired.to_string())
                    {
                        return Err(e);
                    }
                    if e.to_string().contains(&DmApiError::SystemBusy.to_string()) {
                        return Err(e);
                    }
                }
            }
            // 重试间隔
            tokio::time::sleep(Duration::from_millis(retry_interval)).await;
        }
        Ok(false)
    }

    // 程序入口
    pub async fn run(&self) -> Result<()> {
        info!("正在检查用户信息...");
        let user_info = match self.get_user_info().await {
            Ok(info) => info,
            Err(e) => {
                if e.to_string().contains("FAIL_SYS_SESSION_EXPIRED::Session") {
                    error!("获取用户信息失败, cookie已过期, 请重新登陆!");
                } else {
                    error!("获取用户信息失败, 原因:{:?}", e);
                }

                return Ok(());
            }
        };
        let ticket_id = self.account.ticket.id.clone();
        let perfomr_idx = self.account.ticket.sessions - 1; // 场次索引
        let sku_idx = self.account.ticket.grade - 1; // 票档索引
        let priority_purchase_time = self.account.ticket.priority_purchase_time;

        info!("正在获取演唱会信息...");
        let ticket_info = match self.get_ticket_info(ticket_id.clone()).await {
            Ok(info) => info,
            Err(e) => {
                info!("获取演唱会信息失败, {:?}", e);
                return Err(e);
            }
        };

        let ticket_name = ticket_info
            .detail_view_component_map
            .item
            .static_data
            .item_base
            .item_name;

        let perform_id = ticket_info
            .detail_view_component_map
            .item
            .item
            .perform_bases[perfomr_idx]
            .performs[0]
            .perform_id
            .clone();

        let perform_name = ticket_info
            .detail_view_component_map
            .item
            .item
            .perform_bases[perfomr_idx]
            .performs[0]
            .perform_name
            .clone();

        info!("正在获取场次/票档信息...");
        let perform_info = self.get_perform_info(&ticket_id, &perform_id).await?;
        let sku_id = perform_info.perform.sku_list[sku_idx].sku_id.clone();
        let sku_name = perform_info.perform.sku_list[sku_idx].price_name.clone();
        let item_id = perform_info.perform.sku_list[sku_idx].item_id.clone();

        let start_time_str = ticket_info
            .detail_view_component_map
            .item
            .item
            .sell_start_time_str;

        let mut start_timestamp = ticket_info
            .detail_view_component_map
            .item
            .item
            .sell_start_timestamp
            .parse::<i64>()?;

        let request_time = self.account.request_time;

        if request_time > 0 {
            start_timestamp = request_time
        }

        println!(
            "\r\n\t账号备注: {}\n\t账号昵称: {}\n\t门票名称: {}\n\t场次名称: {}\n\t票档名称: {}\n\t开售时间: {}\n",
            self.account.remark, user_info.nickname, ticket_name, perform_name, sku_name, start_time_str
        );

        let local: DateTime<Local> = Local::now();
        let current_timestamp = local.timestamp_millis();

        match current_timestamp > start_timestamp {
            true => {
                if let Err(e) = self.buy_it_now(&item_id, &sku_id).await {
                    if e.to_string()
                        .contains(&DmApiError::ProductEpired.to_string())
                    {
                        let grace_period_millis =
                            self.account.ticket.pick_up_leaks.grace_period_minutes * 60 * 1000;
                        if (current_timestamp - start_timestamp) > grace_period_millis {
                            return Ok(());
                        }
                        info!("商品已售空, 去捡漏...\n");
                        return self.pick_up_leaks(ticket_id, perform_id).await;
                    }
                }
            }
            false => {
                let res = self.wait_for_buy(start_timestamp, &item_id, &sku_id).await;
                match res {
                    Ok(is_succes) => {
                        if is_succes {
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        if e.to_string().contains("退出") {
                            return Ok(());
                        }
                    }
                };
                if priority_purchase_time > 0 {
                    let start_timestamp = start_timestamp + priority_purchase_time * 60 * 1000;
                    info!("优先购已结束, 等待正式开抢...\n\n");
                    if let Ok(res) = self.wait_for_buy(start_timestamp, &item_id, &sku_id).await {
                        if res {
                            return Ok(());
                        }
                    }
                }
                info!("\t未能抢到票, 去捡漏...");
                self.pick_up_leaks(ticket_id, perform_id).await?;
            }
        };
        Ok(())
    }

    // 立即购买
    pub async fn buy_it_now(&self, item_id: &String, sku_id: &String) -> Result<bool> {
        self.multiple_buy_attempts(item_id, sku_id, None).await
    }

    // 等待开售
    pub async fn wait_for_buy(
        &self,
        start_timestamp: i64,
        item_id: &String,
        sku_id: &String,
    ) -> Result<bool> {
        let (s, r) = async_channel::unbounded::<bool>();

        let interval = self.account.interval;
        let earliest_submit_time = self.account.early_submit_time;

        // 轮询等待开抢
        loop {
            tokio::select! {
                _ = signal::ctrl_c() => {
                    return Err(anyhow!("CTRL-C, 退出程序..."));
                }
                _ = tokio::time::sleep(Duration::from_millis(interval)) => {
                    let local: DateTime<Local> = Local::now();
                    let millis = local.timestamp_millis();
                    let time_left_millis = start_timestamp - millis;
                    if time_left_millis <= earliest_submit_time {
                        let _ = s.send(true).await;
                    }else{
                        let (hours, minutes, seconds) = self.ms_to_hms(time_left_millis);
                        print!("\r\t开抢倒计时:{}小时:{}分钟:{:.3}秒\t", hours, minutes, seconds);
                        let _ = io::stdout().flush();
                    }

                }

                _ = r.recv() => {
                    return self.multiple_buy_attempts(item_id, sku_id, None).await
                }
            }
        }
    }

    // 轮询捡漏
    pub async fn pick_up_leaks(&self, ticket_id: String, perform_id: String) -> Result<()> {
        let pick_up_leaks_times = self.account.ticket.pick_up_leaks.times;
        let mut pick_up_leaks_interval = self.account.ticket.pick_up_leaks.interval;
        if pick_up_leaks_interval < 1000 {
            pick_up_leaks_interval = 1000;
        }
        let pick_up_leaks_grades = self.account.ticket.pick_up_leaks.grades.clone();
        let mut pick_up_leaks_num = self.account.ticket.pick_up_leaks.num;
        if pick_up_leaks_num == 0 {
            pick_up_leaks_num = self.account.ticket.num;
        }

        for i in 0..pick_up_leaks_times {
            print!("\r\t第{}次查询库存, ", i + 1);
            if let Ok(perform_info) = self.get_perform_info(&ticket_id, &perform_id).await {
                for idx in 0..perform_info.perform.sku_list.len() {
                    let sku = &perform_info.perform.sku_list[idx];
                    let grade_idx = idx + 1;
                    // 票挡有库存, 并且在配置中
                    if sku.sku_salable.contains("true")
                        && (pick_up_leaks_grades.is_empty()
                            || pick_up_leaks_grades.contains(&grade_idx))
                    {
                        print!("有余票...");
                        info!("票档:{}, 有库存, 去购买...", sku.price_name);
                        if let Ok(res) = self
                            .multiple_buy_attempts(
                                &perform_info.perform.perform_id,
                                &sku.sku_id,
                                Some(pick_up_leaks_num),
                            )
                            .await
                        {
                            if res {
                                return Ok(());
                            }
                        }
                        break;
                    }
                }
            };
            print!("无余票...");
            let _ = io::stdout().flush();
            tokio::time::sleep(Duration::from_millis(pick_up_leaks_interval)).await;
        }

        Ok(())
    }
}
