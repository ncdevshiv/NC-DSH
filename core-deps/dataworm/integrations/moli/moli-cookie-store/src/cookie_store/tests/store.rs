use super::*;
use url::Url;

#[test]
fn domains() {
    let mut store = CookieStore::default();
    fn domain_cookie_from(domain: &str, request_url: &str) -> Cookie<'static> {
        let cookie_str = format!("cookie1=value1; Domain={domain}");
        Cookie::parse(cookie_str, &test_utils::url(request_url)).unwrap()
    }

    {
        let request_url = test_utils::url("http://foo.example.com");
        inserted!(store.insert(
            domain_cookie_from("example.com", "http://foo.example.com",),
            &request_url,
        ));
        updated!(store.insert(
            domain_cookie_from(".example.com", "http://foo.example.com",),
            &request_url,
        ));
        inserted!(store.insert(
            domain_cookie_from("foo.example.com", "http://foo.example.com",),
            &request_url,
        ));
        updated!(store.insert(
            domain_cookie_from(".foo.example.com", "http://foo.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from("bar.example.com", "http://bar.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from(".bar.example.com", "http://bar.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from("bar.foo.example.com", "http://bar.foo.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from(".bar.foo.example.com", "http://bar.foo.example.com",),
            &request_url,
        ));
    }

    {
        let request_url = test_utils::url("http://bar.example.com");
        updated!(store.insert(
            domain_cookie_from("example.com", "http://foo.example.com",),
            &request_url,
        ));
        updated!(store.insert(
            domain_cookie_from(".example.com", "http://foo.example.com",),
            &request_url,
        ));
        inserted!(store.insert(
            domain_cookie_from("bar.example.com", "http://bar.example.com",),
            &request_url,
        ));
        updated!(store.insert(
            domain_cookie_from(".bar.example.com", "http://bar.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from("foo.example.com", "http://foo.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from(".foo.example.com", "http://foo.example.com",),
            &request_url,
        ));
    }
    {
        let request_url = test_utils::url("http://example.com");
        updated!(store.insert(
            domain_cookie_from("example.com", "http://foo.example.com",),
            &request_url,
        ));
        updated!(store.insert(
            domain_cookie_from(".example.com", "http://foo.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from("foo.example.com", "http://foo.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from(".foo.example.com", "http://foo.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from("bar.example.com", "http://bar.example.com",),
            &request_url,
        ));
        domain_mismatch!(store.insert(
            domain_cookie_from(".bar.example.com", "http://bar.example.com",),
            &request_url,
        ));
    }
}

#[test]
fn http_only() {
    let mut store = CookieStore::default();
    let c = Cookie::parse(
        "cookie1=value1; HttpOnly",
        &test_utils::url("http://example.com/foo/bar"),
    )
    .unwrap();
    non_http_scheme!(store.insert(c, &test_utils::url("ftp://example.com/foo/bar"),));
}

#[test]
fn from_cookies_advances_creation_and_access_indices() {
    let one_url = test_utils::url("https://one.example/app");
    let two_url = test_utils::url("https://two.example/app");
    let three_url = test_utils::url("https://three.example/app");

    let mut one = Cookie::parse("a=1; Path=/app; Secure", &one_url)
        .expect("cookie should parse")
        .into_owned();
    one.set_creation_index(4);
    one.touch_with_access_index(9);

    let mut two = Cookie::parse("b=1; Path=/app; Secure", &two_url)
        .expect("cookie should parse")
        .into_owned();
    two.set_creation_index(7);
    two.touch_with_access_index(13);

    let cookies = vec![Ok::<_, CookieError>(one), Ok(two)];
    let mut store = CookieStore::from_cookies(cookies, false).expect("store should load");

    inserted!(store.insert_response_cookie_str("c=1; Path=/app; Secure", &three_url,));

    let inserted_cookie = store
        .get("three.example", "/app", "c")
        .expect("new cookie should exist");
    assert_eq!(inserted_cookie.creation_index(), 8);
    assert_eq!(inserted_cookie.last_access_index(), 14);
}

#[test]
fn from_cookies_include_expired_advances_indices_from_loaded_tombstones() {
    let one_url = test_utils::url("https://one.example/app");
    let two_url = test_utils::url("https://two.example/app");

    let mut expired = Cookie::parse("a=1; Path=/app; Secure; Max-Age=0", &one_url)
        .expect("cookie should parse")
        .into_owned();
    expired.set_creation_index(11);
    expired.touch_with_access_index(21);

    let cookies = vec![Ok::<_, CookieError>(expired)];
    let mut store = CookieStore::from_cookies(cookies, true).expect("store should load");

    inserted!(store.insert_response_cookie_str("b=1; Path=/app; Secure", &two_url,));

    let inserted_cookie = store
        .get("two.example", "/app", "b")
        .expect("new cookie should exist");
    assert_eq!(inserted_cookie.creation_index(), 12);
    assert_eq!(inserted_cookie.last_access_index(), 22);
}

#[test]
fn load_all_advances_creation_and_access_indices() {
    let encoded = concat!(
        "https://one.example/app\t",
        "a=1; Path=/app; Secure\t",
        "4\t9\n",
        "https://two.example/app\t",
        "b=1; Path=/app; Secure; Max-Age=0\t",
        "7\t13\n",
    );

    let mut store = CookieStore::load_all(encoded.as_bytes(), |line| {
        let mut parts = line.split('\t');
        let request_url = Url::parse(parts.next().expect("line should include request url"))
            .expect("request url should parse");
        let cookie_str = parts.next().expect("line should include cookie string");
        let creation_index = parts
            .next()
            .expect("line should include creation index")
            .parse::<u64>()
            .expect("creation index should parse");
        let access_index = parts
            .next()
            .expect("line should include access index")
            .parse::<u64>()
            .expect("access index should parse");
        let mut cookie = Cookie::parse(cookie_str, &request_url)
            .expect("cookie should parse")
            .into_owned();
        cookie.set_creation_index(creation_index);
        cookie.touch_with_access_index(access_index);
        Ok::<_, CookieError>(cookie)
    })
    .expect("store should load all cookies");

    let inserted_url = test_utils::url("https://three.example/app");
    inserted!(store.insert_response_cookie_str("c=1; Path=/app; Secure", &inserted_url,));

    let inserted_cookie = store
        .get("three.example", "/app", "c")
        .expect("new cookie should exist");
    assert_eq!(inserted_cookie.creation_index(), 8);
    assert_eq!(inserted_cookie.last_access_index(), 14);
}

#[test]
fn clear() {
    let mut store = CookieStore::default();
    inserted!(add_cookie(
        &mut store,
        "cookie1=value1",
        "http://example.com/foo/bar",
        Some(test_utils::in_days(1)),
        None,
    ));
    assert!(
        store
            .iter_any()
            .any(|c| c.name_value() == ("cookie1", "value1")),
        "did not find expected cookie1=value1 cookie in store"
    );
    store.clear();
    assert!(
        store.iter_any().count() == 0,
        "found unexpected cookies in cleared store"
    );
}

#[test]
fn add_and_get() {
    let mut store = CookieStore::default();
    assert!(store.get("example.com", "/foo", "cookie1").is_none());

    inserted!(add_cookie(
        &mut store,
        "cookie1=value1",
        "http://example.com/foo/bar",
        None,
        None,
    ));
    assert!(store.get("example.com", "/foo/bar", "cookie1").is_none());
    assert!(store.get("example.com", "/foo", "cookie2").is_none());
    assert!(store.get("example.org", "/foo", "cookie1").is_none());
    assert!(store.get("example.com", "/foo", "cookie1").unwrap().value() == "value1");

    updated!(add_cookie(
        &mut store,
        "cookie1=value2",
        "http://example.com/foo/bar",
        None,
        None,
    ));
    assert!(store.get("example.com", "/foo", "cookie1").unwrap().value() == "value2");

    inserted!(add_cookie(
        &mut store,
        "cookie2=value3",
        "http://example.com/foo/bar",
        None,
        None,
    ));
    assert!(store.get("example.com", "/foo", "cookie1").unwrap().value() == "value2");
    assert!(store.get("example.com", "/foo", "cookie2").unwrap().value() == "value3");

    inserted!(add_cookie(
        &mut store,
        "cookie3=value4; HttpOnly",
        "http://example.com/foo/bar",
        None,
        None,
    ));
    assert!(store.get("example.com", "/foo", "cookie1").unwrap().value() == "value2");
    assert!(store.get("example.com", "/foo", "cookie2").unwrap().value() == "value3");
    assert!(store.get("example.com", "/foo", "cookie3").unwrap().value() == "value4");

    non_http_scheme!(add_cookie(
        &mut store,
        "cookie3=value5",
        "ftp://example.com/foo/bar",
        None,
        None,
    ));
    assert!(store.get("example.com", "/foo", "cookie1").unwrap().value() == "value2");
    assert!(store.get("example.com", "/foo", "cookie2").unwrap().value() == "value3");
    assert!(store.get("example.com", "/foo", "cookie3").unwrap().value() == "value4");
}

#[test]
fn matches() {
    let store = make_match_store();
    check_matches!(&store);
}

#[test]
fn some_non_https_uris_are_secure() {
    let secure_uris = vec![
        "http://localhost",
        "http://localhost:1234",
        "http://127.0.0.1",
        "http://127.0.0.2",
        "http://127.1.0.1",
        "http://[::1]",
    ];
    for secure_uri in secure_uris {
        let mut store = CookieStore::default();
        inserted!(add_cookie(
            &mut store,
            "cookie1=1a; Secure",
            secure_uri,
            None,
            None,
        ));
        matches_are(&store, secure_uri, vec!["cookie1=1a"]);
    }
}

#[test]
fn domain_collisions() {
    let mut store = CookieStore::default();
    inserted!(add_cookie(
        &mut store,
        "cookie1=1a",
        "http://foo.bus.example.com/",
        None,
        None,
    ));
    inserted!(add_cookie(
        &mut store,
        "cookie1=1b",
        "http://bus.example.com/",
        None,
        None,
    ));
    inserted!(add_cookie(
        &mut store,
        "cookie2=2a; Domain=bus.example.com",
        "http://foo.bus.example.com/",
        None,
        None,
    ));
    inserted!(add_cookie(
        &mut store,
        "cookie2=2b; Domain=example.com",
        "http://bus.example.com/",
        None,
        None,
    ));
    matches_are(
        &store,
        "http://foo.bus.example.com/",
        vec!["cookie1=1a", "cookie2=2a", "cookie2=2b"],
    );
    matches_are(
        &store,
        "http://bus.example.com/",
        vec!["cookie1=1b", "cookie2=2a", "cookie2=2b"],
    );
    matches_are(&store, "http://example.com/", vec!["cookie2=2b"]);
    matches_are(&store, "http://foo.example.com/", vec!["cookie2=2b"]);
}

#[test]
fn path_collisions() {
    let mut store = CookieStore::default();
    inserted!(add_cookie(
        &mut store,
        "cookie3=3a",
        "http://bus.example.com/foo/bar/",
        None,
        None,
    ));
    inserted!(add_cookie(
        &mut store,
        "cookie3=3b",
        "http://bus.example.com/foo/",
        None,
        None,
    ));
    inserted!(add_cookie(
        &mut store,
        "cookie4=4a; Path=/foo/bar/",
        "http://bus.example.com/",
        None,
        None,
    ));
    inserted!(add_cookie(
        &mut store,
        "cookie4=4b; Path=/foo/",
        "http://bus.example.com/",
        None,
        None,
    ));
    matches_are(
        &store,
        "http://bus.example.com/foo/bar/",
        vec!["cookie3=3a", "cookie3=3b", "cookie4=4a", "cookie4=4b"],
    );
    matches_are(
        &store,
        "http://bus.example.com/foo/bar",
        vec!["cookie3=3a", "cookie3=3b", "cookie4=4b"],
    );
    matches_are(
        &store,
        "http://bus.example.com/foo/ba",
        vec!["cookie3=3b", "cookie4=4b"],
    );
    matches_are(
        &store,
        "http://bus.example.com/foo/",
        vec!["cookie3=3b", "cookie4=4b"],
    );
    matches_are(&store, "http://bus.example.com/foo", vec!["cookie3=3b"]);
    matches_are(&store, "http://bus.example.com/fo", vec![]);
    matches_are(&store, "http://bus.example.com/", vec![]);
    matches_are(&store, "http://bus.example.com", vec![]);
}
