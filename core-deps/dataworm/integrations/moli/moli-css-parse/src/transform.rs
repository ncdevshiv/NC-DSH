pub use style::moli_transform::{CssTransformFunction, parse_transform_function_list};

#[cfg(test)]
mod tests {
    use super::parse_transform_function_list;

    #[test]
    fn transform_functions_split_comma_and_whitespace_arguments() {
        let functions =
            parse_transform_function_list("translate(calc(10px + 2px) 20px) matrix(1 0 0 1 5 6)")
                .unwrap();

        assert_eq!(functions[0].name, "translate");
        assert_eq!(functions[0].arguments, ["calc(10px + 2px)", "20px"]);
        assert_eq!(functions[1].name, "matrix");
        assert_eq!(functions[1].arguments, ["1", "0", "0", "1", "5", "6"]);

        let comma = parse_transform_function_list("translate(10px, 20px)").unwrap();
        assert_eq!(comma[0].arguments, ["10px", "20px"]);
    }

    #[test]
    fn transform_functions_reject_empty_or_mixed_argument_separators() {
        assert!(parse_transform_function_list("").is_none());
        assert!(parse_transform_function_list("/**/").is_none());
        assert!(parse_transform_function_list("translate()").is_none());
        assert!(parse_transform_function_list("translate(10px,)").is_none());
        assert!(parse_transform_function_list("translate(10px 20px, 30px)").is_none());
    }
}
