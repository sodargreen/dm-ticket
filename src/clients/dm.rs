use crate::models::{ticket::TicketInfoParams, DmRes, DmToken};
use anyhow::Result;

use super::token::TokenClient;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client,
};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use regex::Regex;



#[derive(Debug)]
pub struct DmClient {
    pub client: Client,
    pub token_client: TokenClient,
    pub token: DmToken,
    pub bx_token: String,
    pub content: Vec<String>,
}

// 获取token
pub async fn get_token(cookie: &str) -> Result<DmToken> {
    let mut headers = HeaderMap::new();
    let url = "https://mtop.damai.cn/";

    headers.append("origin", HeaderValue::from_str(url)?);
    headers.append("referer", HeaderValue::from_str(url)?);
    headers.append("cookie", HeaderValue::from_str(cookie)?);
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .cookie_store(true)
        .http2_prior_knowledge()
        .build()?;

    let mut token = DmToken {
        enc_token: "".to_string(),
        token_with_time: "".to_string(),
        token: "".to_string(),
    };

    let url = "https://mtop.damai.cn/h5/mtop.damai.wireless.search.broadcast.list/1.0/?";
    let params = TicketInfoParams::build()?;
    let response = client.get(url).form(&params).send().await?;

    for cookie in response.cookies() {
        if cookie.name() == "_m_h5_tk" {
            token.token_with_time = cookie.value().to_string();
            token.token = token.token_with_time.split('_').collect::<Vec<_>>()[0].to_string();
        }
        if cookie.name() == "_m_h5_tk_enc" {
            token.enc_token = cookie.value().to_string();
        }
    }
    Ok(token)
}

impl DmClient {
    // 初始化请求客户端
    pub async fn new(cookie: String) -> Result<Self> {
        let token_client = TokenClient::new()?;

        // let bx_token = token_client.get_bx_token().await?;
        let bx_token = "G146C2A2D9BA7A13397838F85A22D954E1C1AC33BD3A4B0D426".into();
        let token = get_token(&cookie).await?;
        let file = File::open("../dm-ticket/控制台.txt")?;
        let reader = BufReader::new(file);
    
        let mut content: Vec<String> = Vec::new();
    
        let log_regex = Regex::new(r"\[Log\] (.+) \(dm\.vue, line \d+\)").unwrap();
    
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Some(capture) = log_regex.captures(&line) {
                    let captured_text = capture.get(1).unwrap().as_str().to_string();
                    content.push(captured_text);
                }
            }
        }
    
        // // 打印保存的文本内容
        // for (index, _) in content.iter().enumerate() {
        //     println!("{}: {}", index, content[index]);
        // }

        let mut headers = HeaderMap::new();

        let base_url = "https://mtop.damai.cn/";
        headers.append("origin", HeaderValue::from_str(base_url)?);
        headers.append("referer", HeaderValue::from_str(base_url)?);

        headers.append(
            "cookie",
            HeaderValue::from_str(
                format!(
                    "{};_m_h5_tk_enc={};_m_h5_tk={};",
                    &cookie, token.enc_token, token.token_with_time
                )
                .as_str(),
            )?,
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .http2_prior_knowledge()
            .user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 13_2_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/13.0.3")
            .use_rustls_tls()
            .build()?;
        Ok(Self {
            client,
            token,
            token_client,
            bx_token,
            content,
        })
    }

    // 请求API
    pub async fn request(&self, url: &str, mut params: Value, data: Value, attempt:usize) -> Result<DmRes> {
        let s = format!(
            "{}&{}&{}&{}",
            self.token.token,
            params["t"].as_str().unwrap(),
            params["appKey"].as_str().unwrap(),
            serde_json::to_string(&data)?,
        );

        let sign = format!("{:?}", md5::compute(s));

        params["sign"] = sign.into();

        params["bx-umidtoken"] = self.bx_token.clone().into();
        params["bx-ua"] = "225!/680iizWooizUDt3LjLp6XUoIBX32cLHV3iF9v6Of0p73gpbvvuqJO/jJXaJNqdqMTy9XFLHMHwVpkYo/AW2AqEsV6xVz8KslT13KSt/Dgb7MSmHJy2RsAonLkWRAKfalP7EUHVO0fjrZIpCWVkeO8GejndEHCRIYqpWCEAHXjAEZpkKP39ULhl0+/poQzadDI74fodCbU4KjcI4+yYTbkZW8hulQE0dDMXhfidCbU4KjcI4oL3jDGHz1el9QEp59sKTrfZ0FU4KjcfhbTbc5pX4seoRuooS49fxMkKBoWlkP2IoyV4SbWNRAbCpku3GehXyS6LPb3SdGxfboLijDlH+fej9Qz0dMh6OGdAC5pnPx2knhxw/ytdM84aqWuiqygKIfvBLoFRf6r7neLWFgvNp84UWWu6vVw5LfO5CbU4KjxIhoL3qFl/+f4G0QHkmZJPL3DIAnNJ/VKhDn1aQK8Fx3vSwJAdtUANnAraVq7LmK40rxtZLlUlrXRC9xZSFlgsOwURC65GtQx13UpNPcfxmv16HnMTs2gwZ16lo6HaQ8HN60Xn+U+zhEhceEvhyolEENFAfXfr6PWqesjs1vUdNbKPn3ojmWLWzqGukuXked2em0wFs5nQGi4J5XiB09bRsrjzK+ENRYVuCo7ktHpgpEKzgxmEEaH5xlrxOMhYX1B7qpWbjpCIjVOHU6rK7XyX26cwweTF5ihPAidXAudT+SQ9EFqJ3pKw6j5HSkxpPGO0hSutQ6Q52auFuwM9oyYO0B/Nie6MkhmWt21ZfMn8W5gbonY/YsyEEIW5JyJ4do/siRCrQi+W+nY0CmXSKNx2OrXj78IK58i+j2NJllzjx//LEf3BTn+nXiZimVpT97ieFeAbpWdHMjVXL4k9mxfM6BGUB0sup1nR1JT6EjjHsfPOSDmG9kLzgi+HYyincOxt+o9ZWNrud0IMUl0sFWTrcauGEnbg9KdrwwQoFAVYvIP7D4N41yZ/nhqIiy7f6Tjk5FibO6s6CC5Rqeimlet/dmsCmKL/S9tlMnDPdAl5Sre0FaI2tVYWbvsq=".into();
        // params["bx-ua"] = self.token_client.get_bx_ua().await?.into();
        let form = json!({
            "data": serde_json::to_string(&data)?,
        });

        let response = self
            .client
            .post(url)
            .query(&params)
            .form(&form)
            .send()
            .await?;

        let data = response.json::<DmRes>().await?;

        Ok(data)
    }
}
