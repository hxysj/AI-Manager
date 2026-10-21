use super::*;

// 仅扫描当前接口所在的最多 /24 范围，避免把大网段和虚拟网卡变成无界探测。
fn scan_addresses(interfaces: &[(Ipv4Addr, u8)]) -> Vec<Ipv4Addr> {
    let mut result = std::collections::BTreeSet::new();
    for &(ip, prefix) in interfaces {
        if !(ip.is_private() || ip.is_link_local()) || prefix == 0 || prefix >= 31 {
            continue;
        }
        let mask = u32::MAX << (32 - prefix.max(24));
        let network = u32::from(ip) & mask;
        let broadcast = network | !mask;
        for address in network + 1..broadcast {
            let address = Ipv4Addr::from(address);
            if !interfaces.iter().any(|(own, _)| *own == address) {
                result.insert(address);
            }
        }
    }
    result.into_iter().collect()
}

pub(super) async fn scan(
    registry: LanShareServerRegistry,
    paths: AppPaths,
    app: tauri::AppHandle,
    own_id: String,
) {
    let Ok(interfaces) = interfaces() else { return };
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .http1_only()
        .build()
    else {
        return;
    };
    let auth = TransportAuth {
        sender: own_id.clone(),
        receiver: String::new(),
        secret: String::new(),
        pairing_id: String::new(),
    };
    let mut requests = futures_util::stream::iter(scan_addresses(&interfaces))
        .map(|ip| {
            let client = client.clone();
            let auth = auth.clone();
            async move {
                // 主端口被其他程序占用时，覆盖服务端实际使用的二十个备用端口。
                // 连接被拒绝表示主机可达；无响应的主机不再逐个探测后续端口。
                for port in DEFAULT_PORT..=DEFAULT_PORT + 20 {
                    match tokio::time::timeout(
                        Duration::from_millis(600),
                        tokio::net::TcpStream::connect((ip, port)),
                    )
                    .await
                    {
                        Ok(Ok(stream)) => drop(stream),
                        Ok(Err(error)) if error.kind() == ErrorKind::ConnectionRefused => continue,
                        _ => return None,
                    }
                    let endpoint = Peer {
                        ip: ip.to_string(),
                        port,
                        ..Peer::default()
                    };
                    let response = tokio::time::timeout(
                        Duration::from_millis(600),
                        secure_call(&client, &endpoint, &auth, "info", json!({}), &[], None),
                    )
                    .await;
                    let Ok(Ok(response)) = response else { continue };
                    let Ok(mut peer) = serde_json::from_value::<Peer>(response) else {
                        continue;
                    };
                    if peer.id == auth.sender || !is_native_id(&peer.id) || peer.name.len() > 128 {
                        continue;
                    }
                    peer.ip = endpoint.ip;
                    peer.port = endpoint.port;
                    peer.last_seen_at = now_millis();
                    return Some(peer);
                }
                None
            }
        })
        .buffer_unordered(16);
    let mut emitted_at = tokio::time::Instant::now() - Duration::from_secs(1);
    while let Some(candidate) = requests.next().await {
        let Some(peer) = candidate else { continue };
        let trusted = {
            let mut runtime = registry.inner.lock().await;
            // 主动扫描可重新找到已删除的设备，只有完成重新配对才恢复信任。
            let trusted = runtime.native.authenticate(&peer.id);
            if trusted.is_none()
                && (runtime.native.nearby.len() < 100
                    || runtime.native.nearby.contains_key(&peer.id))
            {
                runtime.native.nearby.insert(peer.id.clone(), peer.clone());
            }
            trusted
        };
        if let Some(trusted) = trusted {
            schedule_peer_check(&registry, &paths, Some(app.clone()), peer, trusted).await;
        }
        if emitted_at.elapsed() >= Duration::from_millis(200) {
            emit_native_changed(&app, &registry).await;
            emitted_at = tokio::time::Instant::now();
        }
    }
    emit_native_changed(&app, &registry).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_scan_is_bounded_deduplicated_and_excludes_own_addresses() {
        let addresses = scan_addresses(&[
            ("192.168.1.10".parse().unwrap(), 16),
            ("192.168.1.11".parse().unwrap(), 24),
            ("127.0.0.1".parse().unwrap(), 8),
            ("8.8.8.8".parse().unwrap(), 24),
        ]);
        assert_eq!(addresses.len(), 252);
        assert!(addresses.iter().all(|ip| ip.octets()[..3] == [192, 168, 1]));
        assert!(!addresses.contains(&"192.168.1.10".parse().unwrap()));
        assert!(!addresses.contains(&"192.168.1.11".parse().unwrap()));
        assert_eq!(
            scan_addresses(&[("192.168.1.10".parse().unwrap(), 28)]).len(),
            13
        );
        assert!(scan_addresses(&[("192.168.1.10".parse().unwrap(), 32)]).is_empty());
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
                if *prefix < 31 {
                    assert!(!scan_addresses(&addresses).contains(&ip));
                }
            }
        }
    }
}
