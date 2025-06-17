use std::os::fd::{AsFd, AsRawFd};

use nix::sys::{
    time::TimeSpec,
    timerfd::{ClockId, Expiration, TimerFd, TimerFlags, TimerSetTimeFlags},
};
use tokio::io::unix::AsyncFd;

use crate::metal::error::MetalError;

// High-resolution timer
pub struct Timer {
    timer: AsyncFd<WrapperTimer>,
}

pub struct WrapperTimer(pub TimerFd);

impl AsRawFd for WrapperTimer {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.0.as_fd().as_raw_fd()
    }
}

impl Timer {
    pub fn new() -> Result<Self, MetalError> {
        Ok(Self {
            timer: AsyncFd::new(WrapperTimer(TimerFd::new(
                ClockId::CLOCK_MONOTONIC,
                TimerFlags::TFD_NONBLOCK,
            )?))?,
        })
    }

    /// Sleep for the given duration
    ///
    /// Look [`sleep_until`](Self::sleep_until) for more information
    pub async fn sleep(&mut self, duration: std::time::Duration) -> Result<(), MetalError> {
        self.sleep_until(std::time::Instant::now() + duration).await
    }

    /// Sleep until the given timestamp
    ///
    /// This is using dichotomic call to [`sleep_with_timer`](Self::sleep_with_timer) to get closer to the target time
    /// and then using active (yielding) wait to get more precise, especially for micro seconds wait
    pub async fn sleep_until(&mut self, timestamp: std::time::Instant) -> Result<(), MetalError> {
        while std::time::Instant::now() < timestamp {
            let duration = timestamp.duration_since(std::time::Instant::now());
            let duration = duration / 2;
            if duration.as_micros() > 10 {
                self.sleep_with_timer(duration).await?;
            } else {
                std::hint::spin_loop();
                // tokio::task::yield_now().await;
            }
        }
        Ok(())
    }

    /// Sleep using a timer file
    ///
    /// This tend to be imprecise but less resource consuming than more precise methods
    pub async fn sleep_with_timer(
        &mut self,
        duration: std::time::Duration,
    ) -> Result<(), MetalError> {
        // Set TimerFd to 0 will disable it. We need to handle this case.
        if duration.as_nanos() == 0 {
            return Ok(());
        }
        self.timer.get_mut().0.set(
            Expiration::OneShot(TimeSpec::from_duration(duration)),
            TimerSetTimeFlags::empty(),
        )?;

        let mut buf = [0; 16];
        loop {
            let mut guard = self.timer.readable().await?;
            match guard
                .try_io(|timer| Ok(nix::unistd::read(timer.get_ref().as_raw_fd(), &mut buf)?))
            {
                Ok(timer) => match timer {
                    Ok(_) => return Ok(()),
                    Err(e) => return Err(MetalError::from(e)),
                },
                Err(_would_block) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest::rstest]
    #[tokio::test]
    async fn test(#[values(0, 1, 2, 3, 4, 5)] sample: u32) {
        let mut timer = Timer::new().unwrap();
        // let mut error_total = std::time::Duration::ZERO;
        // let mut total = std::time::Duration::ZERO;
        let avg = 10_u64.pow(sample);
        let range = (avg / 2).max(1)..(2 * avg);
        let nb = 5 * 10_u32.pow(6 - sample);
        for _ in 0..nb {
            let duration = std::time::Duration::from_micros(rand::random_range(range.clone()));
            // total += duration;

            let start = std::time::Instant::now();
            timer.sleep(duration).await.unwrap();
            let elapsed = start.elapsed();
            assert!(elapsed >= duration);
            // error_total += elapsed;
        }
    }
}
