use super::*;

impl DomHost {
    pub fn table_body_elements(&self, table: DomHandle) -> Vec<DomHandle> {
        if !self.is_html_element_named(table, "table") {
            return Vec::new();
        }
        self.child_handles(table)
            .filter(|handle| self.is_html_element_named(*handle, "tbody"))
            .collect()
    }

    pub fn table_section_row_elements(&self, section: DomHandle) -> Vec<DomHandle> {
        if !is_html_table_section(self, section) {
            return Vec::new();
        }
        self.child_handles(section)
            .filter(|handle| self.is_html_element_named(*handle, "tr"))
            .collect()
    }

    pub fn table_row_cell_elements(&self, row: DomHandle) -> Vec<DomHandle> {
        if !self.is_html_element_named(row, "tr") {
            return Vec::new();
        }
        self.child_handles(row)
            .filter(|handle| {
                self.is_html_element_named(*handle, "td")
                    || self.is_html_element_named(*handle, "th")
            })
            .collect()
    }

    pub fn table_row_elements(&self, table: DomHandle) -> Vec<DomHandle> {
        if !self.is_html_element_named(table, "table") {
            return Vec::new();
        }

        let mut head_rows = Vec::new();
        let mut body_rows = Vec::new();
        let mut foot_rows = Vec::new();
        for child in self.child_handles(table) {
            if self.is_html_element_named(child, "thead") {
                head_rows.extend(self.table_section_row_elements(child));
            } else if self.is_html_element_named(child, "tfoot") {
                foot_rows.extend(self.table_section_row_elements(child));
            } else if self.is_html_element_named(child, "tbody") {
                body_rows.extend(self.table_section_row_elements(child));
            } else if self.is_html_element_named(child, "tr") {
                body_rows.push(child);
            }
        }
        head_rows.extend(body_rows);
        head_rows.extend(foot_rows);
        head_rows
    }
}

fn is_html_table_section(host: &DomHost, handle: DomHandle) -> bool {
    host.is_html_element_named(handle, "thead")
        || host.is_html_element_named(handle, "tbody")
        || host.is_html_element_named(handle, "tfoot")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_host() -> DomHost {
        DomHost::from_dom(NativeDom::new_html(
            url::Url::parse("https://example.test/").expect("valid test url"),
        ))
    }

    #[test]
    fn table_rows_follow_html_collection_order_without_nested_rows() {
        let mut host = test_host();
        let table = host.create_element("table");
        let head = host.create_element("thead");
        let body = host.create_element("tbody");
        let foot = host.create_element("tfoot");
        let orphan = host.create_element("tr");
        let head_row = host.create_element("tr");
        let body_row = host.create_element("tr");
        let foot_row = host.create_element("tr");
        let nested_div = host.create_element("div");
        let nested_row = host.create_element("tr");

        assert!(host.append_child(table, orphan));
        assert!(host.append_child(table, foot));
        assert!(host.append_child(table, body));
        assert!(host.append_child(table, head));
        assert!(host.append_child(head, head_row));
        assert!(host.append_child(body, body_row));
        assert!(host.append_child(foot, foot_row));
        assert!(host.append_child(table, nested_div));
        assert!(host.append_child(nested_div, nested_row));

        assert_eq!(
            host.table_row_elements(table),
            vec![head_row, orphan, body_row, foot_row]
        );
    }

    #[test]
    fn row_cells_include_only_direct_html_cells() {
        let mut host = test_host();
        let row = host.create_element("tr");
        let td = host.create_element("td");
        let th = host.create_element("th");
        let div = host.create_element("div");
        let nested_td = host.create_element("td");
        let foreign_td = host
            .create_element_ns(Some("urn:test"), "td")
            .expect("foreign td");

        assert!(host.append_child(row, td));
        assert!(host.append_child(row, div));
        assert!(host.append_child(div, nested_td));
        assert!(host.append_child(row, foreign_td));
        assert!(host.append_child(row, th));

        assert_eq!(host.table_row_cell_elements(row), vec![td, th]);
    }
}
