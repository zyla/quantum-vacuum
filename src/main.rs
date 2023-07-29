use anyhow::anyhow;
use embedded_hal::pwm::SetDutyCycle;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_hal::ledc::Resolution;
use esp_idf_hal::ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::prelude::*;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};
use esp_idf_sys::{self as _};
use log::*; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use std::cmp::max;
use std::io::Write;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::os::fd::{FromRawFd, IntoRawFd};
use std::time::Duration;

const SSID: &str = env!("WIFI_SSID");
const PASSWORD: &str = env!("WIFI_PASS");

fn main() {
    match real_main() {
        Ok(_) => {}
        Err(e) => {
            error!("real_main() failed: {:?}", e);
        }
    }
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn real_main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();
    // Bind the log crate to the ESP Logging facilities
    EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let timer_driver = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default()
            .resolution(Resolution::Bits12)
            .frequency(50.Hz().into()),
    )?;

    let mut left_forward = LedcDriver::new(
        peripherals.ledc.channel0,
        &timer_driver,
        peripherals.pins.gpio1,
    )?;
    let mut left_backward = LedcDriver::new(
        peripherals.ledc.channel1,
        &timer_driver,
        peripherals.pins.gpio2,
    )?;
    let mut right_forward = LedcDriver::new(
        peripherals.ledc.channel2,
        &timer_driver,
        peripherals.pins.gpio6,
    )?;
    let mut right_backward = LedcDriver::new(
        peripherals.ledc.channel3,
        &timer_driver,
        peripherals.pins.gpio7,
    )?;

    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;

    info!("Wifi DHCP info: {:?}", ip_info);

    let listener = TcpListener::bind("0.0.0.0:1380")?;

    for stream in listener.incoming() {
        match handle_client(
            stream?,
            &mut left_forward,
            &mut left_backward,
            &mut right_forward,
            &mut right_backward,
        ) {
            Ok(_) => {}
            Err(e) => {
                error!("{}", e);
            }
        }
    }

    Ok(())
}

fn handle_client<A: SetDutyCycle, B: SetDutyCycle, C: SetDutyCycle, D: SetDutyCycle>(
    stream: TcpStream,
    left_forward: &mut A,
    left_backward: &mut B,
    right_forward: &mut C,
    right_backward: &mut D,
) -> anyhow::Result<()> {
    let mut left = 0;
    let mut right = 0;

    let fd = stream.into_raw_fd();
    let read_stream = unsafe { TcpStream::from_raw_fd(fd) };
    let mut write_stream = unsafe { TcpStream::from_raw_fd(fd) };
    let mut reader = BufReader::new(read_stream);
    let mut line = String::new();
    while reader.read_line(&mut line)? > 0 {
        println!("line: {}", line);
        let mut s = line.split(char::is_whitespace);
        match (
            s.next().and_then(|x| x.parse::<i32>().ok()),
            s.next().and_then(|x| x.parse::<i32>().ok()),
        ) {
            (Some(l), Some(r)) => {
                left = l;
                right = r;
            }
            _ => {
                writeln!(write_stream, "invalid command")?;
            }
        }
        writeln!(write_stream, "l={left} r={right}")?;
        left_forward
            .set_duty_cycle_fraction(dbg!(forward(left)), 100)
            .map_err(|_| anyhow!("set_duty_cycle"))?;
        left_backward
            .set_duty_cycle_fraction(dbg!(backward(left)), 100)
            .map_err(|_| anyhow!("set_duty_cycle"))?;
        right_forward
            .set_duty_cycle_fraction(dbg!(forward(right)), 100)
            .map_err(|_| anyhow!("set_duty_cycle"))?;
        right_backward
            .set_duty_cycle_fraction(dbg!(backward(right)), 100)
            .map_err(|_| anyhow!("set_duty_cycle"))?;

        line.clear();
    }
    println!("EOF");
    Ok(())
}

fn forward(val: i32) -> u16 {
    max(0, val) as u16
}

fn backward(val: i32) -> u16 {
    forward(-val)
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD.into(),
        channel: None,
    });

    wifi.set_configuration(&wifi_configuration)?;

    wifi.start()?;
    info!("Wifi started");

    wifi.connect()?;
    info!("Wifi connected");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(())
}
