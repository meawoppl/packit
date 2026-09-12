//! "How close is this to the record?" bubble: a side length as a percentage of
//! the proven optimum or best known side for the same `n`. 100% matches the
//! reference; lower is better.

use yew::prelude::*;

/// Relative difference below which a side counts as matching the reference.
/// Comfortably above the server's validation slack, so a validated packing of
/// a proven optimum reads as a match rather than as beating it.
const MATCH_EPS: f64 = 1e-7;

#[derive(Properties, PartialEq)]
pub struct BenchmarkProps {
    /// Current container side.
    pub side: f64,
    /// Literature side for this `n`, if one is loaded.
    pub reference_side: Option<f64>,
    /// Whether the reference is a proven optimum rather than a best known packing.
    pub proven: bool,
    /// Whether `side` belongs to a packing that passed overlap and containment checks.
    pub validated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tone {
    /// Live, unchecked scene at or above the reference.
    Live,
    /// Validated and above the reference.
    Near,
    /// Validated and equal to the reference.
    Match,
    /// Validated and below a best known (unproven) reference.
    Record,
    /// Below the reference without validation, or below a proven optimum.
    Suspect,
    /// No reference loaded.
    Missing,
}

impl Tone {
    fn class(self) -> &'static str {
        match self {
            Tone::Live => "is-live",
            Tone::Near => "is-near",
            Tone::Match => "is-match",
            Tone::Record => "is-record",
            Tone::Suspect => "is-suspect",
            Tone::Missing => "is-missing",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Assessment {
    pub percent: Option<f64>,
    pub headline: String,
    pub detail: String,
    pub tone: Tone,
}

/// Classify a side against the reference. Only a validated packing below an
/// unproven best known value is reported as a record candidate; anything else
/// below the reference is flagged as suspect (overlaps, or a bug).
pub fn assess(side: f64, reference: Option<f64>, proven: bool, validated: bool) -> Assessment {
    let Some(reference) = reference.filter(|r| r.is_finite() && *r > 0.0) else {
        return Assessment {
            percent: None,
            headline: "No reference".into(),
            detail: "No literature value is loaded for this n".into(),
            tone: Tone::Missing,
        };
    };
    let label = if proven {
        "proven optimum"
    } else {
        "best known"
    };
    let status = if validated {
        "validated"
    } else {
        "live · unchecked"
    };
    let ratio = side / reference;
    let gap = ratio - 1.0;
    let (headline, tone) = if gap < -MATCH_EPS {
        if validated && !proven {
            ("New record candidate".to_string(), Tone::Record)
        } else if validated {
            (format!("Below the {label}? Check it"), Tone::Suspect)
        } else {
            ("Overlaps likely".to_string(), Tone::Suspect)
        }
    } else if gap <= MATCH_EPS {
        let tone = if validated { Tone::Match } else { Tone::Live };
        (format!("Matches the {label}"), tone)
    } else {
        let tone = if validated { Tone::Near } else { Tone::Live };
        (format!("{:.3}% above the {label}", gap * 100.0), tone)
    };
    Assessment {
        percent: Some(ratio * 100.0),
        headline,
        detail: format!("of {label} {reference:.6} · {status}"),
        tone,
    }
}

#[function_component(Benchmark)]
pub fn benchmark(props: &BenchmarkProps) -> Html {
    let a = assess(
        props.side,
        props.reference_side,
        props.proven,
        props.validated,
    );
    let percent = a
        .percent
        .map_or_else(|| "—".to_string(), |p| format!("{p:.3}%"));
    html! {
        <div class={classes!("pg-benchmark", a.tone.class())} role="status" aria-live="polite">
            <span class="pg-benchmark-pct">{ percent }</span>
            <span class="pg-benchmark-text">
                <strong>{ a.headline }</strong>
                <small>{ a.detail }</small>
            </span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIVE: f64 = 2.707_106_781_186_547_5;

    #[test]
    fn missing_reference() {
        let a = assess(3.0, None, false, false);
        assert_eq!((a.percent, a.tone), (None, Tone::Missing));
        assert_eq!(assess(3.0, Some(0.0), true, true).tone, Tone::Missing);
    }

    #[test]
    fn live_scene_above_reference() {
        let a = assess(3.0, Some(FIVE), true, false);
        assert_eq!(a.tone, Tone::Live);
        assert!((a.percent.unwrap() - 110.8199).abs() < 1e-3);
        assert!(a.headline.contains("above the proven optimum"));
        assert!(a.detail.ends_with("live · unchecked"));
    }

    #[test]
    fn validated_match_within_tolerance() {
        let a = assess(FIVE * (1.0 + 2e-10), Some(FIVE), true, true);
        assert_eq!(a.tone, Tone::Match);
        assert_eq!(a.headline, "Matches the proven optimum");
    }

    #[test]
    fn record_only_when_validated_and_unproven() {
        assert_eq!(assess(3.87, Some(3.877), false, true).tone, Tone::Record);
        // Unvalidated scenes below the reference are overlapping, not records.
        let live = assess(3.87, Some(3.877), false, false);
        assert_eq!(live.tone, Tone::Suspect);
        assert_eq!(live.headline, "Overlaps likely");
        // Nothing valid can beat a proven optimum.
        assert_eq!(assess(2.7, Some(FIVE), true, true).tone, Tone::Suspect);
    }
}
