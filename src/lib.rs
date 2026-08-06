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
    gpio::{AnyPin, Level, Output, OutputConfig},
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
}

pub fn init_display<'d>(config: DisplayConfig<'d>, delay: &mut Delay) -> Display<'d> {
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

#[doc(hidden)]
pub use esp_hal as _esp_hal;
#[doc(hidden)]
pub use static_cell as _static_cell;

#[macro_export]
macro_rules! setup_backlight {
    ($ledc_peripheral:expr, $bl_pin:expr, $channel_name:ident) => {
        let $channel_name = {
            use $crate::_esp_hal::ledc::channel::ChannelIFace as _;
            use $crate::_esp_hal::ledc::timer::TimerIFace as _;

            static LEDC_CELL: $crate::_static_cell::StaticCell<
                $crate::_esp_hal::ledc::Ledc<'static>,
            > = $crate::_static_cell::StaticCell::new();
            let ledc = LEDC_CELL.init($crate::_esp_hal::ledc::Ledc::new($ledc_peripheral));

            ledc.set_global_slow_clock($crate::_esp_hal::ledc::LSGlobalClkSource::APBClk);

            static TIMER_CELL: $crate::_static_cell::StaticCell<
                $crate::_esp_hal::ledc::timer::Timer<'static, $crate::_esp_hal::ledc::LowSpeed>,
            > = $crate::_static_cell::StaticCell::new();

            let mut t = ledc.timer::<$crate::_esp_hal::ledc::LowSpeed>(
                $crate::_esp_hal::ledc::timer::Number::Timer0,
            );
            t.configure($crate::_esp_hal::ledc::timer::config::Config {
                duty: $crate::_esp_hal::ledc::timer::config::Duty::Duty8Bit,
                clock_source: $crate::_esp_hal::ledc::timer::LSClockSource::APBClk,
                frequency: $crate::_esp_hal::time::Rate::from_khz(5),
            })
            .unwrap();

            let timer = TIMER_CELL.init(t);

            let mut channel = ledc.channel::<$crate::_esp_hal::ledc::LowSpeed>(
                $crate::_esp_hal::ledc::channel::Number::Channel0,
                $bl_pin,
            );

            channel
                .configure($crate::_esp_hal::ledc::channel::config::Config {
                    timer: &*timer,
                    duty_pct: 50,
                    drive_mode: $crate::_esp_hal::gpio::DriveMode::PushPull,
                })
                .unwrap();

            channel
        };
    };
}
