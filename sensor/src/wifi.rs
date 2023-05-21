
use anyhow::bail;
use embedded_svc::wifi::*;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    netif::{EspNetif, EspNetifWait},
    wifi::{EspWifi, WifiWait},
};
use std::net::Ipv4Addr;
use std::time::Duration;
use log::info;


pub fn wifi_sta_start(wifi: &mut Box<EspWifi>, sysloop: &EspSystemEventLoop) -> anyhow::Result<()> {
    // wifi.stop()?;
    wifi.set_configuration(&embedded_svc::wifi::Configuration::Client(
        embedded_svc::wifi::ClientConfiguration {
            ssid: "harpoland".into(),
            password: "alcachofatoxica".into(),
            // channel: Some(1), //channel,
            ..Default::default()
        },
    ))
    .expect("Error configurando wifi sta");

    wifi.start()?;

    info!("Starting wifi...");

    if !WifiWait::new(sysloop)?
        .wait_with_timeout(Duration::from_secs(20), || wifi.is_started().unwrap())
    {
        bail!("Wifi did not start");
    }

    info!("Connecting wifi...");

    wifi.connect()?;

    if !EspNetifWait::new::<EspNetif>(wifi.sta_netif(), sysloop)?.wait_with_timeout(
        Duration::from_secs(20),
        || {
            wifi.is_connected().unwrap()
                && wifi.sta_netif().get_ip_info().unwrap().ip != Ipv4Addr::new(0, 0, 0, 0)
        },
    ) {
        bail!("Wifi did not connect or did not receive a DHCP lease");
    }

    let ip_info = wifi.sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    println!("Wifi sta activado {}", wifi.is_connected().unwrap());
    Ok(())
}

pub fn wifi_ap_start(wifi: &mut Box<EspWifi>, sysloop: &EspSystemEventLoop) -> anyhow::Result<()> {
    wifi.set_configuration(&embedded_svc::wifi::Configuration::AccessPoint(
        embedded_svc::wifi::AccessPointConfiguration {
            ssid: "aptest".into(),
            channel: 1,
            ..Default::default()
        },
    ))
    .expect("Error configurando wifi ap");

    wifi.start().expect("No se puede empezar el wifi");

    info!("Starting wifi...");

    // let sysloop = EspSystemEventLoop::take()?;
    if !WifiWait::new(&sysloop)?
        .wait_with_timeout(Duration::from_secs(20), || wifi.is_started().unwrap())
    {
        bail!("Wifi did not start");
    }
    Ok(())
    // info!("Connecting wifi...");
    // println!("Connecting wifi... ***");

    // wifi.connect()?;
    // Ok(())
}
