pub use tinymetrics::{Counter, Gauge};

use std::fmt;
use tinymetrics::{CounterFamily, FmtLabels, GaugeFamily, MetricBuilder, MetricFamily};

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SensorMetrics {
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub temp: GaugeFamily<'static, TEMP_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub co2: GaugeFamily<'static, CO2_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub eco2: GaugeFamily<'static, ECO2_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub rel_humidity: GaugeFamily<'static, HUMIDITY_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub abs_humidity: GaugeFamily<'static, HUMIDITY_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub pressure: GaugeFamily<'static, PRESSURE_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub gas_resistance: GaugeFamily<'static, VOC_RESISTANCE_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub tvoc: GaugeFamily<'static, TVOC_METRICS, SensorLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub pm_conc: GaugeFamily<'static, PM_CONC_METRICS, DiameterLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub pm_count: GaugeFamily<'static, PM_COUNT_METRICS, DiameterLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub sensor_errors: CounterFamily<'static, SENSORS, SensorLabel>,
}
macro_rules! count_features {
    ($($feature:literal),*) => {{
        let mut n = 0;
        $(#[cfg(feature = $feature)] {
            n += 1;
        })*
        n
    }}

}
pub const TEMP_METRICS: usize = count_features!("scd30", "scd40", "scd41", "bme680", "sht41");
pub const CO2_METRICS: usize = count_features!("scd30", "scd40", "scd41");
pub const ECO2_METRICS: usize = count_features!("sgp30", "bme680", "ens160");
pub const HUMIDITY_METRICS: usize = count_features!("bme680", "scd40", "scd41", "scd30", "sht41");
pub const PRESSURE_METRICS: usize = count_features!("bme680");
pub const VOC_RESISTANCE_METRICS: usize = count_features!("bme680");
pub const TVOC_METRICS: usize = count_features!("sgp30", "bme680", "ens160");
pub const PM_CONC_METRICS: usize = count_features!("pmsa003i") * 3;
pub const PM_COUNT_METRICS: usize = count_features!("pmsa003i") * 6;
pub const SENSORS: usize =
    count_features!("scd30", "scd40", "scd41", "sgp30", "bme680", "ens160", "sht41", "pmsa003i");

#[derive(Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct SensorLabel(pub &'static str);

#[derive(Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct DiameterLabel(pub &'static str);

impl SensorMetrics {
    pub const fn new() -> Self {
        Self {
            temp: MetricBuilder::new("temperature_degrees_celcius")
                .with_help("Temperature in degrees Celcius.")
                .with_unit("celcius")
                .build_labeled::<_, SensorLabel, TEMP_METRICS>(),
            co2: MetricBuilder::new("co2_ppm")
                .with_help("CO2 in parts per million (ppm).")
                .with_unit("ppm")
                .build_labeled::<_, SensorLabel, CO2_METRICS>(),
            eco2: MetricBuilder::new("eco2_ppm")
                .with_help("VOC equivalent CO2 (eCO2) calculated by a tVOC sensor, in parts per million (ppm).")
                .with_unit("ppm")
                .build_labeled::<_, SensorLabel, ECO2_METRICS>(),
            rel_humidity: MetricBuilder::new("humidity_percent")
                .with_help("Relative humidity (RH) percentage.")
                .with_unit("percent")
                .build_labeled::<_, SensorLabel, HUMIDITY_METRICS>(),
            abs_humidity: MetricBuilder::new("absolute_humidity_grams_m3")
                .with_help("Absolute humidity in grams per cubic meter.")
                .with_unit("g/m^3")
                .build_labeled::<_, SensorLabel, HUMIDITY_METRICS>(),
            pressure: MetricBuilder::new("pressure_hpa")
                .with_help("Barometric pressure, in hectopascals (hPa).")
                .with_unit("hPa")
                .build_labeled::<_, SensorLabel, PRESSURE_METRICS>(),
            gas_resistance: MetricBuilder::new("gas_resistance_ohms")
                .with_help("BME680 VOC sensor resistance, in Ohms.")
                .with_unit("Ohms")
                .build_labeled::<_, SensorLabel, VOC_RESISTANCE_METRICS>(),
            tvoc: MetricBuilder::new("tvoc_ppb")
                .with_help("Total Volatile Organic Compounds (VOC) in parts per billion (ppb)")
                .with_unit("ppb")
                .build_labeled::<_, SensorLabel, TVOC_METRICS>(),
            pm_conc: MetricBuilder::new("pm_concentration_ug_m3")
                .with_help("Particulate matter concentration in ug/m^3")
                .with_unit("ug/m^3")
                .build_labeled::<_, DiameterLabel, PM_CONC_METRICS>(),
            pm_count: MetricBuilder::new("pm_count")
                .with_help("Particulate matter count per 0.1L of air.")
                .with_unit("particulates per 0.1L")
                .build_labeled::<_, DiameterLabel, PM_COUNT_METRICS>(),
            sensor_errors: MetricBuilder::new("sensor_error_count")
                .with_help("Count of I2C errors that occurred while talking to a sensor")
                .build_labeled::<_, SensorLabel, SENSORS>(),
        }
    }

    pub fn fmt_metrics(&self, f: &mut impl fmt::Write) -> fmt::Result {
        self.temp.fmt_metric(f)?;
        self.co2.fmt_metric(f)?;
        self.eco2.fmt_metric(f)?;
        self.rel_humidity.fmt_metric(f)?;
        self.abs_humidity.fmt_metric(f)?;
        self.pressure.fmt_metric(f)?;
        self.gas_resistance.fmt_metric(f)?;
        self.tvoc.fmt_metric(f)?;
        self.pm_conc.fmt_metric(f)?;
        self.pm_count.fmt_metric(f)?;
        self.sensor_errors.fmt_metric(f)?;
        Ok(())
    }
}

impl Default for SensorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SensorMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_metrics(f)
    }
}

// === impl Label ===

impl FmtLabels for SensorLabel {
    fn fmt_labels(&self, writer: &mut impl core::fmt::Write) -> core::fmt::Result {
        write!(writer, "sensor=\"{}\"", self.0)
    }
}

impl FmtLabels for DiameterLabel {
    fn fmt_labels(&self, writer: &mut impl core::fmt::Write) -> core::fmt::Result {
        write!(writer, "diameter=\"{}\",sensor=\"PMSA003I\"", self.0)
    }
}

#[cfg(feature = "serde")]
fn serialize_metric<S, M, L, const METRICS: usize>(
    metric: &MetricFamily<M, METRICS, L>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    M: serde::Serialize,
    L: serde::Serialize,
{
    use serde::Serialize;
    metric.metrics().serialize(serializer)
}
