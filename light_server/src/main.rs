use std::path::PathBuf;

use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    #[arg(default_value = "8080")]
    port: String,
}

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    println!("starting web server");
    HttpServer::new(move || {
        App::new()
            .service(get_device_command)
            .service(get_device_config)
            .service(post_device_config)
            .service(get_group_configuration)
            .service(post_group_configuration)
            .service(get_event_configuration)
            .service(post_event_configuration)
            .service(post_event)
    })
    .bind(format!("0.0.0.0:8080"))?
    .run()
    .await
}

fn get_work_dir() -> PathBuf {
    std::env::current_dir().unwrap()
}

// Device poll location
#[get("/v1/device/{device_id}/command")]
async fn get_device_command(device_id: web::Path<String>) -> impl Responder {
    //not implemented
    return HttpResponse::NotImplemented().body("Not implemented");
}
#[get("/v1/device/{device_id}/default")]
async fn get_device_command(device_id: web::Path<String>) -> impl Responder {
    //not implemented
    return HttpResponse::NotImplemented().body("Not implemented");
}

// Device configuration
#[get("/v1/management/devices/{device_id}")]
async fn get_device_config(device_id: web::Path<String>) -> impl Responder {
    return HttpResponse::NotImplemented().body("Not implemented");
}
#[post("/v1/management/devices/{device_id}")]
async fn post_device_config(device_id: web::Path<String>) -> impl Responder {
    return HttpResponse::NotImplemented().body("Not implemented");
}

// Group configuration
#[get("/v1/management/groups/{group_id}")]
async fn get_group_configuration(group_id: web::Path<String>) -> impl Responder {
    return HttpResponse::NotImplemented().body("Not implemented");
}
#[post("/v1/management/groups/{group_id}")]
async fn post_group_configuration(group_id: web::Path<String>) -> impl Responder {
    return HttpResponse::NotImplemented().body("Not implemented");
}

// event configuration
#[get("/v1/management/events/{event_type}")]
async fn get_event_configuration(event_type: web::Path<String>) -> impl Responder {
    return HttpResponse::NotImplemented().body("Not implemented");
}
#[post("/v1/management/events/{event_type}")]
async fn post_event_configuration(event_type: web::Path<String>) -> impl Responder {
    return HttpResponse::NotImplemented().body("Not implemented");
}

// post new event
#[post("/v1/event/")]
async fn post_event() -> impl Responder {
    return HttpResponse::NotImplemented().body("Not implemented");
}
