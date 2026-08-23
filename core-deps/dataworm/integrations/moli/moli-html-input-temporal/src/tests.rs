use super::*;

#[test]
fn moli_round_trips_current_temporal_values() {
    assert_eq!(date_input_milliseconds("1970-01-02"), Some(MS_PER_DAY));
    assert_eq!(
        date_input_value_from_milliseconds(MS_PER_DAY),
        Some("1970-01-02".to_owned())
    );

    assert_eq!(time_input_milliseconds("01:02:03.004"), Some(3_723_004.0));
    assert_eq!(
        time_input_value_from_milliseconds(3_723_004.0),
        Some("01:02:03.004".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(3_723_010.0),
        Some("01:02:03.01".to_owned())
    );

    assert_eq!(month_input_number("1971-02"), Some(13.0));
    assert_eq!(
        month_input_value_from_number(13.0),
        Some("1971-02".to_owned())
    );

    assert_eq!(
        datetime_local_input_value_from_milliseconds(
            datetime_local_input_milliseconds("1970-01-02T01:02").unwrap()
        ),
        Some("1970-01-02T01:02".to_owned())
    );

    assert_eq!(
        week_input_value_from_milliseconds(week_input_milliseconds("1970-W01").unwrap()),
        Some("1970-W01".to_owned())
    );
}

#[test]
fn wpt_value_as_number_vectors_are_covered_in_the_shared_crate() {
    for (value, expected) in [
        ("", None),
        ("0000-12-10", None),
        ("2019-00-12", None),
        ("2019-12-00", None),
        ("2019-13-10", None),
        ("2019-02-29", None),
        ("2019-12-10", Some(1_575_936_000_000.0)),
        ("2016-02-29", Some(1_456_704_000_000.0)),
    ] {
        assert_eq!(date_input_milliseconds(value), expected, "date {value}");
    }
    for (number, expected) in [
        (0.0, "1970-01-01"),
        (1_575_936_000_000.0, "2019-12-10"),
        (1_456_704_000_000.0, "2016-02-29"),
    ] {
        assert_eq!(
            date_input_value_from_milliseconds(number),
            Some(expected.to_owned()),
            "date number {number}"
        );
    }

    for (value, expected) in [
        ("", None),
        ("0000-12", None),
        ("2019-00", None),
        ("2019-12", Some(599.0)),
        ("1969-12", Some(-1.0)),
    ] {
        assert_eq!(month_input_number(value), expected, "month {value}");
    }
    assert_eq!(
        month_input_value_from_number(599.0),
        Some("2019-12".to_owned())
    );
    assert_eq!(
        month_input_value_from_number(-1.0),
        Some("1969-12".to_owned())
    );

    for (value, expected) in [
        ("", None),
        ("0000-W50", None),
        ("2019-W00", None),
        ("2019-W60", None),
        ("2019-W50", Some(1_575_849_600_000.0)),
        ("1969-W20", Some(-20_217_600_000.0)),
    ] {
        assert_eq!(week_input_milliseconds(value), expected, "week {value}");
    }
    for (number, expected) in [
        (0.0, "1970-W01"),
        (1_575_849_600_000.0, "2019-W50"),
        (-20_217_600_000.0, "1969-W20"),
    ] {
        assert_eq!(
            week_input_value_from_milliseconds(number),
            Some(expected.to_owned()),
            "week number {number}"
        );
    }

    for (value, expected) in [
        ("", None),
        ("24:00", None),
        ("00:60", None),
        ("00:00", Some(0.0)),
        ("12:00", Some(12.0 * MS_PER_HOUR)),
        ("23:59", Some((23.0 * MS_PER_HOUR) + (59.0 * MS_PER_MINUTE))),
    ] {
        assert_eq!(time_input_milliseconds(value), expected, "time {value}");
    }
    for (number, expected) in [
        (0.0, "00:00"),
        (12.0 * MS_PER_HOUR, "12:00"),
        ((23.0 * MS_PER_HOUR) + (59.0 * MS_PER_MINUTE), "23:59"),
        (
            2.734_333_707_189_448e26_f64.rem_euclid(MS_PER_DAY),
            "10:54:10.944",
        ),
        ((-3600.0 * MS_PER_SECOND).rem_euclid(MS_PER_DAY), "23:00"),
    ] {
        assert_eq!(
            time_input_value_from_milliseconds(number),
            Some(expected.to_owned()),
            "time number {number}"
        );
    }

    assert_eq!(
        datetime_local_input_milliseconds("2019-12-10T00:00"),
        Some(1_575_936_000_000.0)
    );
    assert_eq!(
        datetime_local_input_milliseconds("2019-12-10T12:00"),
        Some(1_575_979_200_000.0)
    );
    for (number, expected) in [
        (1_575_936_000_000.0, "2019-12-10T00:00"),
        (1_575_979_200_000.0, "2019-12-10T12:00"),
        (-MS_PER_DAY, "1969-12-31T00:00"),
    ] {
        assert_eq!(
            datetime_local_input_value_from_milliseconds(number),
            Some(expected.to_owned()),
            "datetime-local number {number}"
        );
    }
    assert_eq!(
        datetime_local_input_value_from_milliseconds(2.734_333_707_189_448e26),
        None
    );
}

#[test]
fn blink_date_components_round_number_to_temporal_components() {
    assert_eq!(
        date_input_value_from_milliseconds(MS_PER_DAY - 0.4),
        Some("1970-01-02".to_owned())
    );
    assert_eq!(
        date_input_value_from_milliseconds(-0.4),
        Some("1970-01-01".to_owned())
    );
    assert_eq!(
        date_input_value_from_milliseconds(-0.5),
        Some("1969-12-31".to_owned())
    );

    assert_eq!(
        datetime_local_input_value_from_milliseconds(MS_PER_DAY - 0.4),
        Some("1970-01-02T00:00".to_owned())
    );
    assert_eq!(
        datetime_local_input_value_from_milliseconds(-0.5),
        Some("1969-12-31T23:59:59.999".to_owned())
    );

    assert_eq!(
        week_input_value_from_milliseconds((4.0 * MS_PER_DAY) - 0.4),
        Some("1970-W02".to_owned())
    );
    assert_eq!(
        week_input_value_from_milliseconds((4.0 * MS_PER_DAY) - 0.6),
        Some("1970-W01".to_owned())
    );

    assert_eq!(
        month_input_value_from_number(0.49),
        Some("1970-01".to_owned())
    );
    assert_eq!(
        month_input_value_from_number(0.5),
        Some("1970-02".to_owned())
    );
    assert_eq!(
        month_input_value_from_number(-0.49),
        Some("1970-01".to_owned())
    );
    assert_eq!(
        month_input_value_from_number(-0.5),
        Some("1969-12".to_owned())
    );
}

#[test]
fn out_of_range_value_as_number_serialization_returns_none_without_panicking() {
    let huge = 2.734_333_707_189_448e26;

    assert_eq!(date_input_value_from_milliseconds(huge), None);
    assert_eq!(datetime_local_input_value_from_milliseconds(huge), None);
    assert_eq!(week_input_value_from_milliseconds(huge), None);
}

#[test]
fn strict_date_and_month_microsyntax_matches_html_input_values() {
    assert!(is_valid_date_input_value(""));
    assert_eq!(date_input_milliseconds("1969-12-31"), Some(-MS_PER_DAY));
    assert_eq!(date_input_milliseconds("1970-01-01"), Some(0.0));
    assert_eq!(
        date_input_milliseconds("2009-12-22"),
        Some(14_600.0 * MS_PER_DAY)
    );
    assert_eq!(
        date_input_milliseconds("0001-01-01"),
        Some(-719_162.0 * MS_PER_DAY)
    );
    assert_eq!(date_input_milliseconds("0000-12-31"), None);
    assert_eq!(
        date_input_milliseconds("2024-02-29"),
        Some(19_782.0 * MS_PER_DAY)
    );
    assert_eq!(date_input_milliseconds("2023-02-29"), None);
    assert_eq!(date_input_milliseconds("2024-2-09"), None);
    assert_eq!(date_input_milliseconds("2024-02-9"), None);
    assert_eq!(date_input_milliseconds("2024-02-09x"), None);

    assert_eq!(month_input_number("1969-01"), Some(-12.0));
    assert_eq!(month_input_number("1969-12"), Some(-1.0));
    assert_eq!(month_input_number("1970-01"), Some(0.0));
    assert_eq!(month_input_number("1970-12"), Some(11.0));
    assert_eq!(month_input_number("1971-01"), Some(12.0));
    assert_eq!(
        month_input_milliseconds("1969-12"),
        date_input_milliseconds("1969-12-01")
    );
    assert_eq!(
        month_input_milliseconds("2019-12"),
        date_input_milliseconds("2019-12-01")
    );
    assert_eq!(
        month_input_value_from_milliseconds(date_input_milliseconds("1969-12-31").unwrap()),
        Some("1969-12".to_owned())
    );
    assert_eq!(
        month_input_value_from_milliseconds(date_input_milliseconds("2016-02-29").unwrap()),
        Some("2016-02".to_owned())
    );
    assert_eq!(month_input_number("0000-01"), None);
    assert_eq!(month_input_number("2024-00"), None);
    assert_eq!(month_input_number("2024-13"), None);
    assert_eq!(month_input_number("2024-1"), None);
    assert_eq!(month_input_number("2024-01-01"), None);
}

#[test]
fn strict_time_microsyntax_and_normalized_serialization_match_servo_behavior() {
    assert!(is_valid_time_input_value(""));
    assert_eq!(time_input_milliseconds("14:59"), Some(53_940_000.0));
    assert_eq!(time_input_milliseconds("14:59:39"), Some(53_979_000.0));
    assert_eq!(time_input_milliseconds("14:59:39.5"), Some(53_979_500.0));
    assert_eq!(time_input_milliseconds("14:59:39.05"), Some(53_979_050.0));
    assert_eq!(time_input_milliseconds("14:59:39.929"), Some(53_979_929.0));
    assert_eq!(time_input_milliseconds("24:00"), None);
    assert_eq!(time_input_milliseconds("12:60"), None);
    assert_eq!(time_input_milliseconds("12:31:60"), None);
    assert_eq!(time_input_milliseconds("12:31:59."), None);
    assert_eq!(time_input_milliseconds("12:31:59.1234"), None);
    assert_eq!(time_input_milliseconds("12:31:59...29"), None);
    assert_eq!(time_input_milliseconds("123:31"), None);
    assert_eq!(time_input_milliseconds("12:311"), None);
    assert_eq!(time_input_milliseconds("12:31:591"), None);

    assert_eq!(
        time_input_value_from_milliseconds(0.0),
        Some("00:00".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(53_940_000.0),
        Some("14:59".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(53_979_500.0),
        Some("14:59:39.5".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(53_979_050.0),
        Some("14:59:39.05".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(MS_PER_DAY),
        Some("00:00".to_owned())
    );
}

#[test]
fn chromium_time_number_serialization_uses_positive_modulo() {
    assert_eq!(
        time_input_value_from_milliseconds(-1.0),
        Some("23:59:59.999".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(-MS_PER_HOUR),
        Some("23:00".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(MS_PER_DAY + 1_234.0),
        Some("00:00:01.234".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(2.734_333_707_189_448e26),
        Some("10:54:10.944".to_owned())
    );
    assert_eq!(time_input_value_from_milliseconds(f64::NAN), None);
    assert_eq!(time_input_value_from_milliseconds(f64::INFINITY), None);
}

#[test]
fn blink_time_number_serialization_floors_fractional_milliseconds() {
    assert_eq!(
        time_input_value_from_milliseconds(0.9),
        Some("00:00".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(1.9),
        Some("00:00:00.001".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(-0.1),
        Some("23:59:59.999".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(MS_PER_DAY - 0.1),
        Some("23:59:59.999".to_owned())
    );
    assert_eq!(
        time_input_value_from_milliseconds(MS_PER_DAY + 999.9),
        Some("00:00:00.999".to_owned())
    );
}

#[test]
fn datetime_local_accepts_html_separators_and_serializes_normalized_time() {
    let expected = Some(MS_PER_DAY + MS_PER_HOUR + (2.0 * MS_PER_MINUTE));
    assert_eq!(
        datetime_local_input_milliseconds("1970-01-02T01:02"),
        expected
    );
    assert_eq!(
        datetime_local_input_milliseconds("1970-01-02 01:02"),
        expected
    );
    assert_eq!(
        datetime_local_input_milliseconds("1970-01-02T01:02:03.500"),
        Some(MS_PER_DAY + MS_PER_HOUR + (2.0 * MS_PER_MINUTE) + 3_500.0)
    );
    assert_eq!(datetime_local_input_milliseconds("1970-01-02t01:02"), None);
    assert_eq!(datetime_local_input_milliseconds("1970-01-02  01:02"), None);
    assert_eq!(datetime_local_input_milliseconds("1970-01-02T24:00"), None);
    assert_eq!(
        datetime_local_input_value_from_milliseconds(
            MS_PER_DAY + MS_PER_HOUR + (2.0 * MS_PER_MINUTE) + 3_500.0
        ),
        Some("1970-01-02T01:02:03.5".to_owned())
    );
}

#[test]
fn wpt_datetime_local_value_sanitization_vectors_are_covered() {
    for (value, expected) in [
        ("", None),
        ("2014-01-01T11:11:11.111", Some("2014-01-01T11:11:11.111")),
        ("2014-01-01 11:11:11.111", Some("2014-01-01T11:11:11.111")),
        ("2014-01-01 11:11", Some("2014-01-01T11:11")),
        ("2014-01-01 00:00:00.000", Some("2014-01-01T00:00")),
        ("2014-01-0 11:11", None),
        ("2014-01-01 11:1", None),
        ("2014-01-01 11:1d1", None),
        ("2014-01-01H11:11", None),
        ("2014-01-01 11:11:", None),
        ("2014-01-01 11-11", None),
        ("2014-01-01 11:11:123", None),
        ("2014-01-01 11:11:12.1234", None),
    ] {
        let sanitized = datetime_local_input_milliseconds(value)
            .and_then(datetime_local_input_value_from_milliseconds);
        assert_eq!(
            sanitized,
            expected.map(str::to_owned),
            "datetime-local {value}"
        );
    }
}

#[test]
fn week_value_as_number_cases_follow_chromium_coverage() {
    assert_eq!(
        week_input_milliseconds("2007-W01"),
        date_input_milliseconds("2007-01-01")
    );
    assert_eq!(
        week_input_milliseconds("2008-W01"),
        date_input_milliseconds("2007-12-31")
    );
    assert_eq!(
        week_input_milliseconds("2003-W01"),
        date_input_milliseconds("2002-12-30")
    );
    assert_eq!(
        week_input_milliseconds("2004-W01"),
        date_input_milliseconds("2003-12-29")
    );
    assert_eq!(
        week_input_milliseconds("2010-W01"),
        date_input_milliseconds("2010-01-04")
    );
    assert_eq!(
        week_input_milliseconds("2005-W01"),
        date_input_milliseconds("2005-01-03")
    );
    assert_eq!(
        week_input_milliseconds("2006-W01"),
        date_input_milliseconds("2006-01-02")
    );

    assert_eq!(
        week_input_value_from_milliseconds(date_input_milliseconds("2010-01-03").unwrap()),
        Some("2009-W53".to_owned())
    );
    assert_eq!(
        week_input_value_from_milliseconds(date_input_milliseconds("2010-01-04").unwrap()),
        Some("2010-W01".to_owned())
    );
    assert_eq!(
        week_input_value_from_milliseconds(date_input_milliseconds("2010-01-10").unwrap()),
        Some("2010-W01".to_owned())
    );
    assert_eq!(
        week_input_value_from_milliseconds(date_input_milliseconds("2010-01-11").unwrap()),
        Some("2010-W02".to_owned())
    );
    assert_eq!(
        week_input_value_from_milliseconds(date_input_milliseconds("2010-12-31").unwrap()),
        Some("2010-W52".to_owned())
    );

    assert_eq!(week_input_milliseconds("0000-W01"), None);
    assert_eq!(week_input_milliseconds("2011-W53"), None);
    assert_eq!(week_input_milliseconds("2004-W54"), None);
    assert_eq!(week_input_milliseconds("2004-W1"), None);
    assert_eq!(week_input_milliseconds("2004-W001"), None);
    assert_eq!(week_input_milliseconds("2004-W01x"), None);
    assert_eq!(
        week_input_value_from_milliseconds(
            date_input_milliseconds("0000-12-31").unwrap_or(f64::NAN)
        ),
        None
    );
    assert_eq!(
        week_input_value_from_milliseconds(date_input_milliseconds("0001-01-01").unwrap()),
        Some("0001-W01".to_owned())
    );
}

#[test]
fn chromium_html_temporal_limits_are_enforced() {
    assert_eq!(
        date_input_milliseconds("0001-01-01"),
        Some(MIN_DATE_INPUT_MILLISECONDS)
    );
    assert_eq!(date_input_milliseconds("0000-12-31"), None);
    assert_eq!(
        date_input_milliseconds("275760-09-13"),
        Some(MAX_DATE_INPUT_MILLISECONDS)
    );
    assert_eq!(date_input_milliseconds("275760-09-14"), None);
    assert_eq!(date_input_milliseconds("275761-01-01"), None);
    assert_eq!(
        date_input_value_from_milliseconds(MIN_DATE_INPUT_MILLISECONDS),
        Some("0001-01-01".to_owned())
    );
    assert_eq!(
        date_input_value_from_milliseconds(MAX_DATE_INPUT_MILLISECONDS),
        Some("275760-09-13".to_owned())
    );
    assert_eq!(
        date_input_value_from_milliseconds(MAX_DATE_INPUT_MILLISECONDS + MS_PER_DAY),
        None
    );

    assert_eq!(month_input_number("0001-01"), Some(-23_628.0));
    assert_eq!(month_input_number("275760-09"), Some(3_285_488.0));
    assert_eq!(month_input_number("275760-10"), None);
    assert_eq!(month_input_number("275761-01"), None);
    assert_eq!(
        month_input_value_from_number(-23_628.0),
        Some("0001-01".to_owned())
    );
    assert_eq!(
        month_input_value_from_number(3_285_488.0),
        Some("275760-09".to_owned())
    );
    assert_eq!(month_input_value_from_number(3_285_489.0), None);
}

#[test]
fn chromium_datetime_and_week_limits_are_enforced() {
    assert_eq!(
        datetime_local_input_milliseconds("275760-09-13T00:00"),
        Some(MAX_DATE_INPUT_MILLISECONDS)
    );
    assert_eq!(
        datetime_local_input_milliseconds("275760-09-13T00:00:00.001"),
        None
    );
    assert_eq!(
        datetime_local_input_milliseconds("275760-09-14T00:00"),
        None
    );
    assert_eq!(
        datetime_local_input_value_from_milliseconds(MAX_DATE_INPUT_MILLISECONDS),
        Some("275760-09-13T00:00".to_owned())
    );
    assert_eq!(
        datetime_local_input_value_from_milliseconds(MAX_DATE_INPUT_MILLISECONDS + 1.0),
        None
    );

    assert_eq!(
        week_input_milliseconds("275760-W37"),
        date_input_milliseconds("275760-09-08")
    );
    assert_eq!(week_input_milliseconds("275760-W38"), None);
    assert_eq!(week_input_milliseconds("275761-W01"), None);
    assert_eq!(
        week_input_value_from_milliseconds(date_input_milliseconds("275760-09-13").unwrap()),
        Some("275760-W37".to_owned())
    );
    assert_eq!(
        week_input_value_from_milliseconds(MAX_DATE_INPUT_MILLISECONDS + MS_PER_WEEK),
        None
    );
}
