#!/usr/bin/env python3
"""
Simple UDP connectivity test for all services

Tests:
1. Brain Advice Bus (45100) - Collector → Brain
2. Brain Decision Bus (45110) - Brain → Executor
3. Mempool Brain Port (45120) - Mempool → Brain
4. Mempool Executor Port (45130) - Mempool → Executor
"""

import socket
import time


def test_port(port: int, name: str) -> bool:
    """Test if a UDP port is listening"""
    try:
        # Create test socket
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(0.5)

        # Try to send test message
        test_msg = b"PING"
        sock.sendto(test_msg, ("127.0.0.1", port))

        # Don't wait for response (UDP is fire-and-forget)
        # Just check if send succeeded
        sock.close()
        return True

    except Exception as e:
        return False


def main():
    print(
        """
╔════════════════════════════════════════════════════════════╗
║              UDP PORT CONNECTIVITY TEST                    ║
╚════════════════════════════════════════════════════════════╝
    """
    )

    ports = [
        (45100, "Brain Advice Bus", "Collector → Brain"),
        (45110, "Brain Decision Bus", "Brain → Executor"),
        (45120, "Mempool Brain Port", "Mempool → Brain"),
        (45130, "Mempool Executor Port", "Mempool → Executor"),
    ]

    print("Testing UDP ports...\n")

    results = []
    for port, name, flow in ports:
        # Check if port can be bound (means service NOT running)
        test_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            test_socket.bind(("127.0.0.1", port))
            test_socket.close()
            status = "❌ NOT LISTENING"
            available = False
        except OSError:
            status = "✅ LISTENING"
            available = True

        print(f"Port {port:5d} ({name:25s}): {status}")
        print(f"           Flow: {flow}")
        print()

        results.append(available)

    # Summary
    print("=" * 60)
    listening = sum(results)
    total = len(results)

    print(f"\n📊 SUMMARY: {listening}/{total} ports listening")

    if listening == total:
        print("✅ All services appear to be running!")
    elif listening > 0:
        print("⚠️  Some services are running, but not all")
    else:
        print("❌ No services detected - make sure to start:")
        print("   1. Brain service (cargo run --release in brain/)")
        print("   2. Executor service (cargo run --release in execution/)")
        print("   3. Mempool watcher (cargo run --release in mempool-watcher/)")

    print()


if __name__ == "__main__":
    main()
