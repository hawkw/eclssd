pub use tinymetrics::{Counter, Gauge};

use eclss_api::SensorName;
use std::fmt;
use tinymetrics::{CounterFamily, FmtLabels, GaugeFamily, MetricBuilder, MetricFamily};

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SensorMetrics {
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub temp_c: GaugeFamily<'static, TEMP_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub co2_ppm: GaugeFamily<'static, CO2_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub eco2_ppm: GaugeFamily<'static, ECO2_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub rel_humidity_percent: GaugeFamily<'static, HUMIDITY_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub abs_humidity_grams_m3: GaugeFamily<'static, HUMIDITY_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub pressure_hpa: GaugeFamily<'static, PRESSURE_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub gas_resistance: GaugeFamily<'static, VOC_RESISTANCE_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub tvoc_ppb: GaugeFamily<'static, TVOC_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub tvoc_iaq_index: GaugeFamily<'static, TVOC_IAQ_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub nox_iaq_index: GaugeFamily<'static, NOX_IAQ_METRICS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    #[serde(skip)]
    pub pm_conc: GaugeFamily<'static, PM_CONC_METRICS, DiameterLabel>,
    // #[cfg_attr(feature = "serde", serde(serialize_with =
    // "serialize_metric"))]
    #[serde(skip)]
    pub pm_count: GaugeFamily<'static, PM_COUNT_METRICS, DiameterLabel>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub sensor_errors: CounterFamily<'static, SENSORS, SensorName>,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_metric"))]
    pub sensor_reset_count: CounterFamily<'static, SENSORS, SensorName>,
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
pub const TEMP_METRICS: usize =
    count_features!("scd30", "scd40", "scd41", "bme680", "sht41", "sen55");
pub const CO2_METRICS: usize = count_features!("scd30", "scd40", "scd41");
pub const ECO2_METRICS: usize = count_features!("sgp30", "bme680", "ens160");
pub const HUMIDITY_METRICS: usize =
    count_features!("bme680", "scd40", "scd41", "scd30", "sht41", "sen55");
pub const PRESSURE_METRICS: usize = count_features!("bme680");
pub const VOC_RESISTANCE_METRICS: usize = count_features!("bme680");
pub const TVOC_METRICS: usize = count_features!("sgp30", "bme680", "ens160");
// IAQ from 1-500
pub const TVOC_IAQ_METRICS: usize = count_features!("sen55", "bme680", "sgp40");
pub const NOX_IAQ_METRICS: usize = count_features!("sen55");
pub const PM_CONC_METRICS: usize =
    // PMSA003I exposes three particulate concentration metrics
    (count_features!("pmsa003i") * 3)
    // SEN5x sensors expose 4 particulate concentration metrics
    + (count_features!("sen55") * 4);
pub const PM_COUNT_METRICS: usize = count_features!("pmsa003i") * 6;
pub const SENSORS: usize = count_features!(
    "scd30", "scd40", "scd41", "sen55", "sgp30", "bme680", "ens160", "sht41", "pmsa003i"
);

#[derive(Debug, Eq, PartialEq, serde::Serialize)]
pub struct DiameterLabel {
    pub diameter: &'static str,
    pub sensor: SensorName,
}

impl SensorMetrics {
    pub const fn new() -> Self {
        Self {
            temp_c: MetricBuilder::new("temperature_degrees_celcius")
                .with_help("Temperature in degrees Celcius.")
                .with_unit("celcius")
                .build_labeled::<_, SensorName, TEMP_METRICS>(),
            co2_ppm: MetricBuilder::new("co2_ppm")
                .with_help("CO2 in parts per million (ppm).")
                .with_unit("ppm")
                .build_labeled::<_, SensorName, CO2_METRICS>(),
            eco2_ppm: MetricBuilder::new("eco2_ppm")
                .with_help("VOC equivalent CO2 (eCO2) calculated by a tVOC sensor, in parts per million (ppm).")
                .with_unit("ppm")
                .build_labeled::<_, SensorName, ECO2_METRICS>(),
            rel_humidity_percent: MetricBuilder::new("humidity_percent")
                .with_help("Relative humidity (RH) percentage.")
                .with_unit("percent")
                .build_labeled::<_, SensorName, HUMIDITY_METRICS>(),
            abs_humidity_grams_m3: MetricBuilder::new("absolute_humidity_grams_m3")
                .with_help("Absolute humidity in grams per cubic meter.")
                .with_unit("g/m^3")
                .build_labeled::<_, SensorName, HUMIDITY_METRICS>(),
            pressure_hpa: MetricBuilder::new("pressure_hpa")
                .with_help("Barometric pressure, in hectopascals (hPa).")
                .with_unit("hPa")
                .build_labeled::<_, SensorName, PRESSURE_METRICS>(),
            gas_resistance: MetricBuilder::new("gas_resistance_ohms")
                .with_help("BME680 VOC sensor resistance, in Ohms.")
                .with_unit("Ohms")
                .build_labeled::<_, SensorName, VOC_RESISTANCE_METRICS>(),
            tvoc_ppb: MetricBuilder::new("tvoc_ppb")
                .with_help("Total Volatile Organic Compounds (VOC) in parts per billion (ppb)")
                .with_unit("ppb")
                .build_labeled::<_, SensorName, TVOC_METRICS>(),
            tvoc_iaq_index: MetricBuilder::new("tvoc_iaq_index")
                .with_help("Total Volatile Organic Compounds (VOC) Indoor Air Quality (IAQ) Index from 0-500")
                .with_unit("IAQ index")
                .build_labeled::<_, SensorName, TVOC_IAQ_METRICS>(),
            nox_iaq_index: MetricBuilder::new("nox_iaq_index")
                .with_help("Nitrogen Oxides (NOx) Indoor Air Quality (IAQ) Index from 0-500")
                .with_unit("IAQ index")
                .build_labeled::<_, SensorName, NOX_IAQ_METRICS>(),
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
                .build_labeled::<_, SensorName, SENSORS>(),

            sensor_reset_count: MetricBuilder::new("sensor_reset_count")
                .with_help("The number of times a sensor was reset successfully")
                .build_labeled::<_, SensorName, SENSORS>(),
        }
    }

    pub fn fmt_metrics(&self, f: &mut impl fmt::Write) -> fmt::Result {
        self.temp_c.fmt_metric(f)?;
        self.co2_ppm.fmt_metric(f)?;
        self.eco2_ppm.fmt_metric(f)?;
        self.rel_humidity_percent.fmt_metric(f)?;
        self.abs_humidity_grams_m3.fmt_metric(f)?;
        self.pressure_hpa.fmt_metric(f)?;
        self.gas_resistance.fmt_metric(f)?;
        self.tvoc_ppb.fmt_metric(f)?;
        self.tvoc_iaq_index.fmt_metric(f)?;
        self.nox_iaq_index.fmt_metric(f)?;
        self.pm_conc.fmt_metric(f)?;
        self.pm_count.fmt_metric(f)?;
        self.sensor_errors.fmt_metric(f)?;
        self.sensor_reset_count.fmt_metric(f)?;
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

impl FmtLabels for DiameterLabel {
    fn fmt_labels(&self, writer: &mut impl core::fmt::Write) -> core::fmt::Result {
        let Self { diameter, sensor } = self;
        write!(writer, "diameter=\"{diameter}\",sensor=\"{sensor}\"")
    }
}

#[cfg(feature = "serde")]
fn serialize_metric<S, M, const METRICS: usize>(
    metric: &MetricFamily<M, METRICS, SensorName>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    M: tinymetrics::Metric + serde::Serialize,
{
    use serde::ser::SerializeSeq;
    let metrics = metric.metrics();
    let mut seq = serializer.serialize_seq(Some(metrics.len()))?;

    for (sensor, value) in metrics.iter() {
        if !value.has_been_recorded() {
            continue;
        }
        #[derive(serde::Serialize)]
        struct SerializeMetric<'metric, M> {
            sensor: &'metric SensorName,
            value: &'metric M,
        }
        seq.serialize_element(&SerializeMetric { sensor, value })?;
    }

    seq.end()
}
