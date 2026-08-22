pub const PROJECT_LICENSE_TEXT: &str = include_str!("../LICENSE");
pub const THIRD_PARTY_LICENSE_TEXT: &str = include_str!("../THIRD_PARTY_LICENSES.txt");

pub fn print_license_report(component: &str) {
    println!("{component} project license\n===============================\n");
    print!("{PROJECT_LICENSE_TEXT}");
    println!("\nThird-party dependency licenses\n================================\n");
    print!("{THIRD_PARTY_LICENSE_TEXT}");
}
