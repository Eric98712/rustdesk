# Wake a manually configured computer from an online LAN host

LAN-discovered peers can already be woken from the RustDesk UI. The `--wol`
command adds one-shot sending by MAC address without a discovery record or UI
session, including explicit broadcast and source IPv4 addresses for a host with
multiple network interfaces.

```text
rustdesk --wol AA:BB:CC:DD:EE:FF
rustdesk --wol AA:BB:CC:DD:EE:FF 192.168.50.255 192.168.50.2
```

The defaults are `255.255.255.255` and `0.0.0.0` (OS-selected source). The command
validates all arguments before sending three UDP port 9 packets, 250 ms apart,
using the existing WoL library. An invalid argument or send error is returned
through the existing native CLI error path. Successful sending is not proof that
the target woke. No service, listener, persistent task or credentials are added.

For an off-site caller, keep a computer online on the target's physical LAN and
reach it through an existing authenticated channel, for example SSH over a VPN:

```sh
ssh wake-host 'rustdesk --wol AA:BB:CC:DD:EE:FF 192.168.50.255 192.168.50.2'
```

Use the target's physical NIC MAC, its LAN's actual broadcast address and an
IPv4 address assigned to the sender's LAN interface. A VPN address is normally
not the source to use for the magic packet. Tailscale does not carry Ethernet
broadcasts; the online LAN host sends the packet locally after receiving the SSH
command. This proposal does not add a remote-wake protocol to RustDesk.

Enable the target's firmware/driver Wake-on-LAN settings and keep the NIC
powered. Sleep, hibernate, full shutdown and wireless wake support differ by
hardware. A wired Windows machine in the motivating deployment passed two S3
wakes and one full S5 shutdown/startup after firmware configuration, using the
earlier Python sender. That is deployment evidence, not hardware acceptance of
this newly ported CLI. A wireless target did not wake in the same deployment.

The online LAN host needs Internet access independent of the sleeping target.
If its VPN/SSH connection relies on Internet sharing from that target, local
scheduled sending can work while off-site wake cannot. Verify actual wake-up
using target boot/resume records and reconnecting, with a person nearby during
the first test. This command does not change firmware, power policy or login.
