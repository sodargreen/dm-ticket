use anyhow::Result;
use dm_ticket::{
    config::{load_global_config, Config},
    ticket,
};
use dotenv::dotenv;
use futures::future::join_all;
use log::{error, warn};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "INFO");
    }

    if env::var("TOKEN_SERVER_URL").is_err() {
        env::set_var("TOKEN_SERVER_URL", "http://127.0.0.1:8080/");
    }

    pretty_env_logger::init();

    let config: Config = match load_global_config() {
        Some(conf) => conf,
        None => {
            error!("加载配置失败, 退出程序...");
            return Ok(());
        }
    };

    warn!("\n特别声明: 
        \n\t1.不得将此项目中任何内容用于违反国家/地区/组织等的法律法规或相关规定的其他用途。
        \n\t2.此项目涉及的数据由使用的个人或组织自行填写，作者不对数据内容负责，包括但不限于数据的真实性、准确性、合法性。
        \n\t3.使用本项目所造成的一切后果，与本项目的所有贡献者无关，由使用的个人或组织完全承担。\n\n\n
        ");

    // let secs = 5;
    // for i in 0..secs {
    //     print!("\r\t{}秒后开始执行程序...\t", secs - i);
    //     tokio::time::sleep(Duration::from_secs(1)).await;
    //     let _ = io::stdout().flush();
    // }
    // println!("\n\n");

    let mut handlers = Vec::new();

    for account in config.accounts.iter() {
        let account = account.clone();
        let handler = tokio::spawn(async move {
            let dm_ticket = ticket::DmTicket::new(account).await.unwrap();
            // let perform_id = String::from("211301573");
            // let ticket_id = String::from("720545258599");
            // let _ = dm_ticket.pick_up_leaks(ticket_id, perform_id).await;
            dm_ticket.run().await.unwrap();
        });
        handlers.push(handler);
    }

    join_all(handlers).await;

    println!("\n\n\n如遇到错误:[哎哟喂,被挤爆啦,请稍后重试], 请不要再重试, 请先检查:
            \t1.是否使用了大厂服务器?
            \t2.cookies是否复制完整?
            \t3.是否频繁运行程序导致触发了频繁验证? 请重新获取cookie之后再尝试!
            \t4.ticket.retry_times不宜设置过大。
            \t5.捡漏轮询间隔(pick_up_leaks.interval)不宜设置过小, 轮询次数(pick_up_leaks.times)不宜设置过大!
            \n\n\n");

    Ok(())
}
