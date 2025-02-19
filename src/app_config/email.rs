use std::env;

use lettre::{Message, SmtpTransport, Transport};
use lettre::message::header;
use lettre::transport::smtp::authentication::Credentials;

pub async fn send_email(title: &str, body: String) {
    // SMTP 服务器地址和端口
    let smtp_server = &env::var("EMAIL_SMTP_SERVER").unwrap_or(String::from("smtp.gmail.com"));
    let smtp_port = env::var("EMAIL_SMTP_PORT").unwrap_or("587".to_string());

    // 发件人和收件人
    let from = env::var("EMAIL_FROM").unwrap_or("xxxxxxxx@gmail.com".to_string());
    let to = env::var("EMAIL_TO").unwrap_or("xxxxxx@163.com".to_string());

    // 发件人邮箱的凭证
    let username = env::var("EMAIL_SEND_USERNAME").unwrap_or("xxxxxxxx@gmail.com".to_string());
    let password = env::var("EMAIL_SEND_PASSWORD").unwrap_or("xxxxxx".to_string());

    // println!("user_name:{}", username);
    // println!("user_password:{}", password);
    // 创建邮件内容
    let email = Message::builder().from(from.parse().unwrap()).to(to.parse().unwrap()).subject(title).header(header::ContentType::TEXT_PLAIN).body(body).unwrap();

    // 设置 SMTP 客户端
    let creds = Credentials::new(username.to_string(), password.to_string());

    let mailer = SmtpTransport::starttls_relay(smtp_server).unwrap().port(smtp_port.parse().unwrap()).credentials(creds).build();

    // 发送邮件
    match mailer.send(&email) {
        Ok(_) => println!("Email sent successfully!"),
        Err(e) => {
            eprintln!("Could not send email: {:?}", e);
            println!("email:{:?}", email);
            println!("mailer:{:?}", mailer)
        }
    }
}
