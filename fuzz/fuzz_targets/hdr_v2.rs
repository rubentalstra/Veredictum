// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
//! The HDR histogram V2 decode path, which re-derives a published performance
//! class from base64 the runner did not produce.
//!
//! A measured record is re-checkable precisely because the latency percentiles
//! come back out of the encoded histogram rather than out of the summary
//! fields beside it: `class_verdict` decodes and re-reads every quantile. So a
//! party publishing a results record hands this decoder an arbitrary byte
//! string and the verdict machinery consumes it, which makes it the one place
//! where third-party bytes reach a binary format reader.
//!
//! The harness base64-encodes the fuzzer's bytes rather than treating them as
//! base64 text: base64 decoding is a well-exercised third-party path, and
//! spending the budget discovering the alphabet would leave the V2 decoder
//! itself barely reached.
//!
//! The property is that a corrupt encoding is a typed refusal, never a panic
//! and never an allocation driven by a length the encoding claims. The decoded
//! histogram is then read the way `class_verdict` reads it.

use base64::Engine;
use libfuzzer_sys::fuzz_target;

use veredictum::perf::OperationMeasurement;

fuzz_target!(|data: &[u8]| {
    let measurement = OperationMeasurement {
        operation: "composition_read".to_owned(),
        requests: 0,
        errors: 0,
        latency_ms_p50: 0.0,
        latency_ms_p90: 0.0,
        latency_ms_p99: 0.0,
        hdr_v2_base64: base64::engine::general_purpose::STANDARD.encode(data),
    };

    let Ok(histogram) = measurement.decode_histogram() else {
        return;
    };

    // The three quantiles the class thresholds are checked against, plus the
    // count the evidence line reports.
    for quantile in [0.50, 0.90, 0.99] {
        let _ = histogram.value_at_quantile(quantile);
    }
    let _ = histogram.len();
    let _ = histogram.max();
});
