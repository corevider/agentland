use std::path::{Path, PathBuf};

/// A certificate for this machine, made here and kept here.
///
/// Not for anybody else's trust: it is what a browser needs before it will open
/// a camera or a microphone on a page. Over plain http both are refused, which
/// is why the phone could show the crew but never speak to it.
///
/// The phone will say it does not recognise the certificate, once, and that is
/// the truth — nobody signed it. What it protects against is somebody else on
/// the network reading the token going past.
pub struct Papers {
    pub certificate: PathBuf,
    pub key: PathBuf,
}

/// Make them if they are not there, and leave them alone if they are: a new
/// certificate every start would mean the same warning every start.
pub fn papers_for(data_dir: &Path, hosts: &[String]) -> anyhow::Result<Papers> {
    let folder = data_dir.join("tls");
    std::fs::create_dir_all(&folder)?;

    let certificate = folder.join("certificate.pem");
    let key = folder.join("key.pem");

    if certificate.is_file() && key.is_file() {
        return Ok(Papers { certificate, key });
    }

    let names = names_for(hosts);
    let made = rcgen::generate_simple_self_signed(names)?;

    std::fs::write(&certificate, made.cert.pem())?;
    std::fs::write(&key, made.key_pair.serialize_pem())?;

    Ok(Papers { certificate, key })
}

/// The names the certificate has to cover: whatever a phone might type, plus
/// the loopback names, and never an empty list — a certificate for nothing is
/// refused by every browser there is.
pub fn names_for(hosts: &[String]) -> Vec<String> {
    let mut names: Vec<String> = hosts
        .iter()
        .map(|held| held.rsplit_once(':').map(|(host, _)| host.to_owned()).unwrap_or_else(|| held.clone()))
        .filter(|held| !held.is_empty() && held != "0.0.0.0")
        .collect();

    names.push("localhost".to_owned());
    names.push("127.0.0.1".to_owned());
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_come_from_the_addresses_without_their_ports() {
        let names = names_for(&["192.168.1.128:9470".into(), "ege:9470".into()]);

        assert!(names.contains(&"192.168.1.128".to_owned()));
        assert!(names.contains(&"ege".to_owned()));
    }

    #[test]
    fn the_loopback_names_are_always_covered() {
        let names = names_for(&[]);

        assert!(names.contains(&"localhost".to_owned()));
        assert!(names.contains(&"127.0.0.1".to_owned()));
    }

    #[test]
    fn the_address_it_was_told_to_bind_is_not_a_name() {
        assert!(!names_for(&["0.0.0.0:9470".into()]).contains(&"0.0.0.0".to_owned()));
    }

    #[test]
    fn papers_made_once_are_not_made_again() {
        let dir = std::env::temp_dir().join("agentland-tls-once");
        let _ = std::fs::remove_dir_all(&dir);

        let first = papers_for(&dir, &["192.168.1.5:9470".into()]).expect("papers");
        let held = std::fs::read_to_string(&first.certificate).expect("a certificate");

        let again = papers_for(&dir, &["192.168.1.5:9470".into()]).expect("papers");

        assert_eq!(std::fs::read_to_string(&again.certificate).unwrap(), held);
    }
}
