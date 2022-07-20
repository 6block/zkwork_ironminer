/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use futures::channel::oneshot;
use log::*;
use ringbuffer::{AllocRingBuffer, RingBuffer, RingBufferExt, RingBufferWrite};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::RwLock,
    task,
    time::{self, Instant},
};

#[derive(Debug)]
pub struct RollingAverage {
    container: AllocRingBuffer<f64>,
    average: f64,
    out_of_date: bool,
}

impl RollingAverage {
    pub fn new(len: usize) -> Self {
        RollingAverage {
            container: AllocRingBuffer::with_capacity(len),
            average: 0.0,
            out_of_date: false,
        }
    }

    pub fn average(&self) -> f64 {
        if !self.out_of_date {
            self.average
        } else {
            let mut sum = 0.0;
            for &v in self.container.iter() {
                sum += v;
            }
            sum / self.container.len() as f64
        }
    }

    pub fn add(&mut self, val: f64) {
        self.out_of_date = true;
        self.container.push(val);
    }

    pub fn reset(&mut self) {
        self.container.clear();
        self.average = 0.0;
        self.out_of_date = false;
    }
}

#[derive(Debug)]
pub struct Meter {
    started: AtomicBool,
    rate_1s: RwLock<RollingAverage>,
    rate_5s: RwLock<RollingAverage>,
    rate_1m: RwLock<RollingAverage>,
    rate_5m: RwLock<RollingAverage>,
    rate_average: RwLock<RollingAverage>,
    count: AtomicU64,
}
impl Meter {
    pub fn new() -> Arc<Self> {
        Arc::new(Meter {
            started: Default::default(),
            rate_1s: RwLock::new(RollingAverage::new(1)),
            rate_5s: RwLock::new(RollingAverage::new(8)),
            rate_1m: RwLock::new(RollingAverage::new(64)),
            rate_5m: RwLock::new(RollingAverage::new(512)),
            rate_average: RwLock::new(RollingAverage::new(128)),
            count: Default::default(),
        })
    }

    pub async fn get_rate_1s(&self) -> f64 {
        self.rate_1s.read().await.average()
    }

    pub async fn get_rate_5s(&self) -> f64 {
        self.rate_5s.read().await.average()
    }

    pub async fn get_rate_1m(&self) -> f64 {
        self.rate_1m.read().await.average()
    }

    pub async fn get_rate_5m(&self) -> f64 {
        self.rate_5m.read().await.average()
    }

    pub async fn get_avg(&self) -> f64 {
        self.rate_average.read().await.average()
    }

    pub async fn add(&self, count: u64) {
        if !self.started.load(Ordering::Relaxed) {
            return;
        }
        self.count.fetch_add(count, Ordering::SeqCst);
        self.rate_average.write().await.add(count as f64);
    }

    pub async fn start(meter: Arc<Meter>) {
        if meter.started.load(Ordering::Relaxed) {
            return;
        }
        meter.started.store(true, Ordering::SeqCst);
        let (router, handler) = oneshot::channel();
        task::spawn(async move {
            let _ = router.send(());
            let mut interval = time::interval(Duration::from_millis(1000));
            let mut last_now = Instant::now();
            loop {
                let _ = interval.tick().await;
                if !meter.started.load(Ordering::Relaxed) {
                    break;
                }
                // update
                let now = Instant::now();
                let count = meter.count.load(Ordering::Relaxed);
                meter.count.fetch_sub(count, Ordering::SeqCst);
                let elapse_ms = now.saturating_duration_since(last_now).as_millis() as u64;
                if elapse_ms == 0 {
                    continue;
                }
                let rate_sec = count / elapse_ms * 1000;
                meter.rate_1s.write().await.add(rate_sec as f64);
                meter.rate_5s.write().await.add(rate_sec as f64);
                meter.rate_1m.write().await.add(rate_sec as f64);
                meter.rate_5m.write().await.add(rate_sec as f64);
                last_now = now;
            }
            debug!("Meter stop.");
        });
        let _ = handler.await;
    }

    pub async fn stop(&self) {
        if !self.started.load(Ordering::Relaxed) {
            return;
        }
        self.started.store(false, Ordering::SeqCst);
        self.count.store(0, Ordering::SeqCst);
    }

    pub fn format(hash_rate: f64) -> String {
        match hash_rate {
            x if x < 1000.0 => format!("{:3.2} H/s", x),
            x if x < 1000000.0 => format!("{:3.2} KH/s", x / 1000.0),
            x if x < 1000000000.0 => format!("{:3.2} MH/s", x / 1000000.0),
            x if x < 1000000000000.0 => format!("{:3.2} GH/s", x / 1000000000.0),
            x if x < 1000000000000000.0 => format!("{:3.2} TH/s", x / 1000000000000.0),
            x => format!("{:.2} PH/s", x / 1000000000000000.0),
        }
    }
}
#[cfg(test)]
mod tests {

    use crate::{Meter, RollingAverage};

    #[test]
    fn test_rolling_average() {
        let mut av_1 = RollingAverage::new(1);
        av_1.add(100.0);
        assert_eq!(100.0, av_1.average());
        av_1.add(200.0);
        assert_eq!(200.0, av_1.average());
        let mut av_2 = RollingAverage::new(2);
        av_2.add(100.0);
        assert_eq!(100.0, av_2.average());
        av_2.add(200.0);
        assert_eq!(150.0, av_2.average());
        av_2.add(300.0);
        assert_eq!(250.0, av_2.average());
    }

    #[test]
    fn test_format() {
        let x = 200.00;
        let format_x = Meter::format(x);
        assert_eq!(format_x, String::from("200.00 H/s"));
        println!("{}", format_x);
        let x = 200000.00;
        let format_x = Meter::format(x);
        assert_eq!(format_x, String::from("200.00 KH/s"));
        println!("{}", format_x);
        let x = 200000000.00;
        let format_x = Meter::format(x);
        assert_eq!(format_x, String::from("200.00 MH/s"));
        println!("{}", format_x);
        let x = 200000000000.00;
        let format_x = Meter::format(x);
        assert_eq!(format_x, String::from("200.00 GH/s"));
        println!("{}", format_x);
        let x = 200000000000000.00;
        let format_x = Meter::format(x);
        assert_eq!(format_x, String::from("200.00 TH/s"));
        println!("{}", format_x);
        let x = 200000000000000000.00;
        let format_x = Meter::format(x);
        assert_eq!(format_x, String::from("200.00 PH/s"));
        println!("{}", format_x);
    }
}
