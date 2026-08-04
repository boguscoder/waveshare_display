#![no_std]

use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    pixelcolor::{Rgb565, RgbColor},
};
use embedded_hal::delay::DelayNs;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    delay::Delay,
    gpio::{AnyPin, Level, Output, OutputConfig, Pin},
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed, channel, channel::ChannelIFace, timer, timer::TimerIFace,
    },
    spi::master::{Config as SpiConfig, Spi},
};
use mipidsi::{
    Builder,
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, Orientation, Rotation},
};
use static_cell::StaticCell;

pub type Display<'d> = mipidsi::Display<
    SpiInterface<'d, ExclusiveDevice<Spi<'d, esp_hal::Blocking>, Output<'d>, Delay>, Output<'d>>,
    ST7789,
    Output<'d>,
>;

pub struct LandscapeDisplay<'a, 'd> {
    pub base: &'a mut Display<'d>,
}

pub const DISPLAY_HEIGHT: i32 = 172;
pub const DISPLAY_WIDTH: i32 = 320;
const DISPLAY_REAL_HEIGHT: i32 = 240;
const OFFSET_Y: i32 = 34;

impl<'a, 'd> DrawTarget for LandscapeDisplay<'a, 'd> {
    type Color = Rgb565;
    type Error = <Display<'d> as DrawTarget>::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let shifted = pixels
            .into_iter()
            .map(|Pixel(point, color)| Pixel(Point::new(point.x, point.y + OFFSET_Y), color));

        self.base.draw_iter(shifted)
    }
}

impl<'a, 'd> OriginDimensions for LandscapeDisplay<'a, 'd> {
    fn size(&self) -> Size {
        Size::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32)
    }
}

pub struct DisplayConfig<'d> {
    pub spi: esp_hal::peripherals::SPI2<'d>,
    pub mosi: AnyPin<'d>,
    pub sclk: AnyPin<'d>,
    pub cs: AnyPin<'d>,
    pub dc: AnyPin<'d>,
    pub rst: AnyPin<'d>,
    pub bl: AnyPin<'d>,
    pub ledc: esp_hal::peripherals::LEDC<'d>,
    pub backlight_duty: u8,
}

static mut BACKLIGHT_CHANNEL: Option<channel::Channel<'static, LowSpeed>> = None;

pub fn set_backlight(duty: u8) {
    unsafe {
        let channel_ptr = &raw mut BACKLIGHT_CHANNEL;
        if let Some(channel) = (*channel_ptr).as_mut() {
            let _ = channel.set_duty(duty);
        }
    }
}

pub fn init_display<'d>(config: DisplayConfig<'d>, delay: &mut Delay) -> Display<'d> {
    let mut ledc = Ledc::new(config.ledc);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

    let mut timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    timer
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty8Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: esp_hal::time::Rate::from_khz(5),
        })
        .unwrap();

    let mut channel = ledc.channel::<LowSpeed>(
        channel::Number::Channel0,
        config.bl.degrade().into_output_signal(),
    );
    channel
        .configure(channel::config::Config {
            timer: &timer,
            duty_pct: config.backlight_duty,
            drive_mode: esp_hal::gpio::DriveMode::PushPull,
        })
        .unwrap();

    unsafe {
        BACKLIGHT_CHANNEL = Some(core::mem::transmute_copy(&channel));
    }

    let mut rst = Output::new(config.rst, Level::Low, OutputConfig::default());
    delay.delay_ms(100u32);
    rst.set_high();
    delay.delay_ms(100u32);

    let spi_config = SpiConfig::default().with_frequency(esp_hal::time::Rate::from_mhz(40));
    let spi = Spi::new(config.spi, spi_config)
        .expect("Failed to init SPI")
        .with_sck(config.sclk)
        .with_mosi(config.mosi);
    let cs = Output::new(config.cs, Level::High, OutputConfig::default());
    let dc = Output::new(config.dc, Level::Low, OutputConfig::default());

    let spi_device = ExclusiveDevice::new(spi, cs, Delay::new()).unwrap();
    static DISPLAY_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();
    let di = SpiInterface::new(spi_device, dc, DISPLAY_BUFFER.init([0; 1024]));

    let mut display = Builder::new(ST7789, di)
        .display_size(DISPLAY_REAL_HEIGHT as u16, DISPLAY_WIDTH as u16)
        .display_offset(0, 0)
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(rst)
        .init(delay)
        .unwrap();

    display
        .set_orientation(Orientation::new().rotate(Rotation::Deg90))
        .unwrap();
    display.clear(Rgb565::BLACK).unwrap();

    display
}
