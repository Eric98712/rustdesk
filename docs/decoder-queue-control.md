# Experimental decoder queue control

This native-client policy reacts to estimated queued decode work rather than
waiting for a large frame-count backlog. It uses existing automatic FPS messages
and display-refresh recovery; compressed delta frames are not discarded or
reordered by the policy. There is no protocol or sender change.

To opt in, close the session and set `enable-decoder-queue-control = 'Y'` in
the `[options]` table of that peer's RustDesk configuration, then reconnect.
Remove that option and reconnect to use the existing controller. There is no
new UI or Web-client implementation in this proposal.

The requested FPS remains the ceiling (5-120, default 30). Available decoder
capacity reserves 10% headroom for direct connections and 20% for relayed
connections, as the existing controller does. The maximum queue work is
estimated as queued frames divided by measured local decode capacity; it is
not network delay, frame age or input-to-display latency.

- More than one queued frame and at least 80 ms of estimated work lowers the
  requested rate. At 250 ms it brakes more strongly.
- A backlog of at least 150 ms requests existing display-refresh recovery,
  at most once every five seconds per display.
- Recovery raises the target by at most 4 fps after three healthy samples and
  at least two seconds since braking. Idle desktops do not probe upward.
- Multiple active displays share the lowest proposed limit. A display with no
  output and no queued work for 15 samples stops limiting active displays.

These are experimental thresholds. This policy limits decoder backlog, not
network congestion or host capture/encoding cost. A previous single-run H.264
comparison measured 54.25 versus 55.05 callbacks/s and no sustained decoder
backlog; it does not establish a causal FPS gain. Stable 60 fps and end-to-end
latency improvement have not been demonstrated. Evaluate repeated overloaded
decoder runs and recovery, display switching and manual-FPS changes before
considering enabling this by default. No diagnostic logging or background
sampler is included.
