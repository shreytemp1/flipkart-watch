use chrono::{DateTime, Utc};

/// Format an amount with western thousands separators, matching Python's
/// `f"{amount:,.0f}"` used by the original implementation.
pub fn format_amount(amount: f64) -> String {
    let rounded = amount.round();
    let negative = rounded.is_sign_negative() && rounded != 0.0;
    let digits = format!("{:.0}", rounded.abs());
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let grouped: String = grouped.chars().rev().collect();
    if negative {
        format!("-{grouped}")
    } else {
        grouped
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Product {
    pub name: String,
    pub path: String,
    pub pid: String,
    pub min_coupon: Option<f64>,
}

impl Product {
    pub fn new(name: impl Into<String>, path: impl Into<String>, pid: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            pid: pid.into(),
            min_coupon: None,
        }
    }

    pub fn with_min_coupon(mut self, min_coupon: f64) -> Self {
        self.min_coupon = Some(min_coupon);
        self
    }

    pub fn url(&self) -> String {
        format!("https://www.flipkart.com{}?pid={}", self.path, self.pid)
    }
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub product: Product,
    pub available: bool,
    pub in_stock: bool,
    pub deliverable: bool,
    pub price: Option<f64>,
    pub mrp: Option<f64>,
    pub title: Option<String>,
    pub unserviceability_reason: Option<String>,
    pub coupon: Option<f64>,
    pub error: Option<String>,
}

impl CheckResult {
    pub fn new(product: Product) -> Self {
        Self {
            product,
            available: false,
            in_stock: false,
            deliverable: false,
            price: None,
            mrp: None,
            title: None,
            unserviceability_reason: None,
            coupon: None,
            error: None,
        }
    }

    pub fn with_error(product: Product, error: impl Into<String>) -> Self {
        Self {
            error: Some(error.into()),
            ..Self::new(product)
        }
    }

    pub fn ok(&self) -> bool {
        self.error.is_none()
    }

    /// Whether the listing carries the coupon the user asked for.
    pub fn coupon_ok(&self) -> bool {
        match self.product.min_coupon {
            None => true,
            Some(minimum) => self.coupon.is_some_and(|coupon| coupon >= minimum),
        }
    }

    /// Whether this listing should trigger an alert (stock + coupon gate).
    pub fn qualifies(&self) -> bool {
        self.available && self.coupon_ok()
    }

    pub fn reason(&self) -> String {
        if let Some(error) = &self.error {
            return error.clone();
        }
        if self.available {
            if let Some(minimum) = self.product.min_coupon {
                if !self.coupon_ok() {
                    return format!("no ₹{} coupon", format_amount(minimum));
                }
            }
            return "available".to_string();
        }
        if !self.in_stock {
            return "out of stock".to_string();
        }
        if !self.deliverable {
            return "not deliverable to this pincode".to_string();
        }
        "unavailable".to_string()
    }

    pub fn price_text(&self) -> String {
        match self.price {
            None => "price unknown".to_string(),
            Some(price) => format!("₹{}", format_amount(price)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunReport {
    pub pincode: String,
    pub address: String,
    pub results: Vec<CheckResult>,
    pub checked_at: DateTime<Utc>,
}

impl RunReport {
    /// Listings that are buyable *and* meet their coupon requirement.
    pub fn available(&self) -> Vec<&CheckResult> {
        self.results
            .iter()
            .filter(|result| result.qualifies())
            .collect()
    }

    pub fn errors(&self) -> Vec<&CheckResult> {
        self.results
            .iter()
            .filter(|result| result.error.is_some())
            .collect()
    }

    pub fn any_available(&self) -> bool {
        self.results.iter().any(CheckResult::qualifies)
    }
}

#[cfg(test)]
mod tests {
    use super::format_amount;

    #[test]
    fn formats_amounts_with_thousands_separators() {
        assert_eq!(format_amount(79999.0), "79,999");
        assert_eq!(format_amount(139999.0), "139,999");
        assert_eq!(format_amount(10000.0), "10,000");
        assert_eq!(format_amount(999.0), "999");
        assert_eq!(format_amount(0.0), "0");
    }
}
