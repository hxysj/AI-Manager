use super::*;

pub(super) fn broadcast_address(ip: Ipv4Addr, prefix: u8) -> Option<Ipv4Addr> {
    if !(ip.is_private() || ip.is_link_local()) || prefix == 0 || prefix >= 31 {
        return None;
    }
    let mask = u32::MAX << (32 - prefix);
    Some(Ipv4Addr::from(u32::from(ip) | !mask))
}

pub(super) fn targets() -> Result<Vec<SocketAddr>, ManagerError> {
    let mut result = vec![SocketAddr::from(([255, 255, 255, 255], DISCOVERY_PORT))];
    for (ip, prefix) in interfaces()? {
        if let Some(broadcast) = broadcast_address(ip, prefix) {
            let target = SocketAddr::from((broadcast, DISCOVERY_PORT));
            if !result.contains(&target) {
                result.push(target);
            }
        }
    }
    Ok(result)
}

#[cfg(windows)]
fn interfaces() -> Result<Vec<(Ipv4Addr, u8)>, ManagerError> {
    use windows_sys::Win32::{
        Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS},
        NetworkManagement::IpHelper::{GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH},
        Networking::WinSock::{AF_INET, SOCKADDR_IN},
    };
    let mut size = 15_000u32;
    loop {
        // 使用对齐内存，系统填充的链表仅在缓冲区有效期间读取。
        let mut buffer = vec![0u64; (size as usize + 7) / 8];
        let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let status =
            unsafe { GetAdaptersAddresses(AF_INET as u32, 0, std::ptr::null(), first, &mut size) };
        if status == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if status == ERROR_NO_DATA {
            return Ok(Vec::new());
        }
        if status != ERROR_SUCCESS {
            return Err(std::io::Error::from_raw_os_error(status as i32).into());
        }
        let mut addresses = Vec::new();
        let mut adapter = first;
        while !adapter.is_null() {
            let current = unsafe { &*adapter };
            if current.OperStatus == 1 {
                let mut unicast = current.FirstUnicastAddress;
                while !unicast.is_null() {
                    let address = unsafe { &*unicast };
                    if !address.Address.lpSockaddr.is_null()
                        && address.Address.iSockaddrLength
                            >= std::mem::size_of::<SOCKADDR_IN>() as i32
                        && unsafe { (*address.Address.lpSockaddr).sa_family } == AF_INET
                    {
                        let socket = unsafe { &*address.Address.lpSockaddr.cast::<SOCKADDR_IN>() };
                        let ip =
                            Ipv4Addr::from(unsafe { socket.sin_addr.S_un.S_addr }.to_ne_bytes());
                        addresses.push((ip, address.OnLinkPrefixLength));
                    }
                    unicast = address.Next;
                }
            }
            adapter = current.Next;
        }
        return Ok(addresses);
    }
}

#[cfg(unix)]
fn interfaces() -> Result<Vec<(Ipv4Addr, u8)>, ManagerError> {
    let mut first = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut first) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut result = Vec::new();
    let mut entry = first;
    while !entry.is_null() {
        let address = unsafe { &*entry };
        if !address.ifa_addr.is_null()
            && !address.ifa_netmask.is_null()
            && unsafe { (*address.ifa_addr).sa_family } as i32 == libc::AF_INET
            && address.ifa_flags & libc::IFF_UP as u32 != 0
        {
            let ip = unsafe { &*address.ifa_addr.cast::<libc::sockaddr_in>() };
            let mask = unsafe { &*address.ifa_netmask.cast::<libc::sockaddr_in>() };
            result.push((
                Ipv4Addr::from(ip.sin_addr.s_addr.to_ne_bytes()),
                u32::from_be(mask.sin_addr.s_addr).count_ones() as u8,
            ));
        }
        entry = address.ifa_next;
    }
    unsafe { libc::freeifaddrs(first) };
    Ok(result)
}

pub(super) fn packet(kind: &str, peer: &Peer) -> Result<Vec<u8>, ManagerError> {
    transport::encrypt(
        "",
        "device-drop-discovery-v2",
        &serde_json::to_vec(&json!({
            "protocol": DISCOVERY_PROTOCOL, "kind": kind, "peer": peer, "sentAt": now_millis()
        }))?,
    )
}

pub(super) fn parse(
    bytes: &[u8],
    source: SocketAddr,
    own_id: &str,
) -> Option<(DiscoveryPacketKind, Peer, u64)> {
    let plaintext = transport::decrypt("", "device-drop-discovery-v2", bytes).ok()?;
    let payload: Value = serde_json::from_slice(&plaintext).ok()?;
    if payload["protocol"] != DISCOVERY_PROTOCOL {
        return None;
    }
    let kind = match payload["kind"].as_str()? {
        "announce" => DiscoveryPacketKind::Announce,
        "probe" => DiscoveryPacketKind::Probe,
        _ => return None,
    };
    let mut peer: Peer = serde_json::from_value(payload["peer"].clone()).ok()?;
    if peer.id == own_id || !is_native_id(&peer.id) || peer.id.len() > 80 || peer.name.len() > 128 {
        return None;
    }
    peer.ip = source.ip().to_string();
    peer_origin(&peer).ok()?;
    let sent_at = payload["sentAt"].as_u64()?;
    if now_millis().abs_diff(sent_at) > 5 * 60 * 1000 {
        return None;
    }
    peer.last_seen_at = now_millis();
    Some((kind, peer, sent_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_uses_real_prefix_including_non_24_subnets() {
        assert_eq!(
            broadcast_address("192.168.12.5".parse().unwrap(), 20)
                .unwrap()
                .to_string(),
            "192.168.15.255"
        );
        assert_eq!(
            broadcast_address("10.2.3.4".parse().unwrap(), 16)
                .unwrap()
                .to_string(),
            "10.2.255.255"
        );
        assert!(broadcast_address("127.0.0.1".parse().unwrap(), 8).is_none());
        assert!(broadcast_address("192.168.1.5".parse().unwrap(), 31).is_none());
        assert!(targets()
            .unwrap()
            .iter()
            .all(|address| address.port() == DISCOVERY_PORT));
    }

    #[cfg(windows)]
    #[test]
    fn windows_enumeration_includes_the_active_ipv4_interface() {
        let addresses = interfaces().unwrap();
        assert!(addresses.iter().all(|(_, prefix)| *prefix <= 32));
        if let Ok(std::net::IpAddr::V4(ip)) = local_ip_address::local_ip() {
            if ip.is_private() || ip.is_link_local() {
                let (_, prefix) = addresses
                    .iter()
                    .find(|(address, _)| *address == ip)
                    .expect("网卡枚举应包含当前局域网地址");
                if let Some(broadcast) = broadcast_address(ip, *prefix) {
                    assert!(targets()
                        .unwrap()
                        .contains(&SocketAddr::from((broadcast, DISCOVERY_PORT))));
                }
            }
        }
    }
}
