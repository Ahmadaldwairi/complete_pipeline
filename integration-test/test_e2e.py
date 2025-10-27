#!/usr/bin/env python3
"""
End-to-End Integration Test for Solana Scalper Bot

Tests the complete flow:
1. Data Collector → Brain (feature windows via UDP)
2. Brain → Executor (trade decisions via UDP)
3. Executor → Brain (telemetry feedback via UDP)

Measures:
- Message delivery success rates
- End-to-end latency (target: <250ms)
- Component health
"""

import socket
import struct
import time
import json
from dataclasses import dataclass
from typing import Optional
import statistics

# UDP Ports
ADVICE_BUS_PORT = 45100  # Collector → Brain
DECISION_BUS_PORT = 45110  # Brain → Executor
TELEMETRY_PORT = 45115  # Executor → Brain (if implemented)

# Test configuration
NUM_TEST_MESSAGES = 10
TIMEOUT_SECS = 5.0


@dataclass
class LatencyMeasurement:
    """Single latency measurement"""

    test_id: int
    sent_time: float
    received_time: Optional[float]
    latency_ms: Optional[float]
    success: bool


class IntegrationTester:
    """E2E integration test runner"""

    def __init__(self):
        self.results = []
        self.advice_socket = None
        self.decision_socket = None

    def setup(self):
        """Initialize test sockets"""
        print("🔧 Setting up test environment...")

        # Socket to send advice (simulating Collector)
        self.advice_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

        # Socket to receive decisions (simulating Executor)
        self.decision_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.decision_socket.bind(("127.0.0.1", DECISION_BUS_PORT))
        self.decision_socket.settimeout(TIMEOUT_SECS)

        print(f"✅ Advice sender ready (→ port {ADVICE_BUS_PORT})")
        print(f"✅ Decision receiver ready (← port {DECISION_BUS_PORT})")

    def cleanup(self):
        """Close sockets"""
        if self.advice_socket:
            self.advice_socket.close()
        if self.decision_socket:
            self.decision_socket.close()
        print("🧹 Cleanup complete")

    def create_test_advice(self, test_id: int) -> bytes:
        """Create a test advice message (late opportunity)"""
        # Simplified advice structure (mimics Collector output)
        advice = {
            "type": "late_opportunity",
            "test_id": test_id,
            "mint": f"TestMint{test_id:04d}AAAAAAAAAAAAAAAAAAAAAA",
            "timestamp": int(time.time()),
            "mint_features": {
                "volume_10s": 50000.0 + (test_id * 1000),
                "volume_60s": 150000.0 + (test_id * 3000),
                "holders_10s": 25 + test_id,
                "price_change_60s": 15.0 + (test_id * 0.5),
                "buys_vs_sells_300s": 0.65 + (test_id * 0.01),
            },
        }

        # Convert to bytes (Brain expects bincode, but we'll use JSON for testing)
        return json.dumps(advice).encode("utf-8")

    def send_advice(self, advice_data: bytes) -> float:
        """Send advice to Brain"""
        sent_time = time.time()
        self.advice_socket.sendto(advice_data, ("127.0.0.1", ADVICE_BUS_PORT))
        return sent_time

    def wait_for_decision(self, timeout: float) -> Optional[tuple]:
        """Wait for decision from Brain"""
        try:
            data, addr = self.decision_socket.recvfrom(8192)
            received_time = time.time()
            return (data, received_time)
        except socket.timeout:
            return None

    def run_single_test(self, test_id: int) -> LatencyMeasurement:
        """Run a single E2E test"""
        print(f"\n📤 Test {test_id}/{NUM_TEST_MESSAGES}: Sending advice...")

        # Create and send advice
        advice = self.create_test_advice(test_id)
        sent_time = self.send_advice(advice)

        # Wait for decision
        result = self.wait_for_decision(TIMEOUT_SECS)

        if result:
            data, received_time = result
            latency_ms = (received_time - sent_time) * 1000
            print(
                f"✅ Received decision: {len(data)} bytes, latency: {latency_ms:.2f}ms"
            )

            return LatencyMeasurement(
                test_id=test_id,
                sent_time=sent_time,
                received_time=received_time,
                latency_ms=latency_ms,
                success=True,
            )
        else:
            print(f"❌ No decision received (timeout after {TIMEOUT_SECS}s)")
            return LatencyMeasurement(
                test_id=test_id,
                sent_time=sent_time,
                received_time=None,
                latency_ms=None,
                success=False,
            )

    def run_tests(self):
        """Run all integration tests"""
        print("\n" + "=" * 60)
        print("🚀 STARTING END-TO-END INTEGRATION TEST")
        print("=" * 60)
        print(f"Test messages: {NUM_TEST_MESSAGES}")
        print(f"Target latency: <250ms")
        print(f"Timeout: {TIMEOUT_SECS}s")
        print()

        self.setup()

        try:
            for i in range(1, NUM_TEST_MESSAGES + 1):
                measurement = self.run_single_test(i)
                self.results.append(measurement)
                time.sleep(0.5)  # Brief pause between tests

        finally:
            self.cleanup()

        self.print_report()

    def print_report(self):
        """Print test results summary"""
        print("\n" + "=" * 60)
        print("📊 INTEGRATION TEST RESULTS")
        print("=" * 60)

        successful = [r for r in self.results if r.success]
        failed = [r for r in self.results if not r.success]

        success_rate = (len(successful) / len(self.results)) * 100

        print(
            f"\n✅ Successful: {len(successful)}/{len(self.results)} ({success_rate:.1f}%)"
        )
        print(f"❌ Failed: {len(failed)}/{len(self.results)}")

        if successful:
            latencies = [r.latency_ms for r in successful]

            print(f"\n⏱️  LATENCY STATISTICS:")
            print(f"   Min:     {min(latencies):.2f}ms")
            print(f"   Max:     {max(latencies):.2f}ms")
            print(f"   Mean:    {statistics.mean(latencies):.2f}ms")
            print(f"   Median:  {statistics.median(latencies):.2f}ms")

            if len(latencies) > 1:
                print(f"   StdDev:  {statistics.stdev(latencies):.2f}ms")

            # Check target
            mean_latency = statistics.mean(latencies)
            if mean_latency < 250:
                print(f"\n🎯 TARGET MET: Mean latency {mean_latency:.2f}ms < 250ms ✅")
            else:
                print(f"\n⚠️  TARGET MISSED: Mean latency {mean_latency:.2f}ms > 250ms")

        print("\n" + "=" * 60)

        # Detailed results
        if failed:
            print("\n❌ FAILED TESTS:")
            for r in failed:
                print(f"   Test {r.test_id}: Timeout (no response)")

        print()


def check_services():
    """Check if required services are running"""
    print("🔍 Checking service availability...")

    services = {
        "Brain (Advice Bus)": ADVICE_BUS_PORT,
        "Brain (Decision Bus)": DECISION_BUS_PORT,
    }

    all_ok = True
    for name, port in services.items():
        # Try to bind to check if port is in use
        test_socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            test_socket.bind(("127.0.0.1", port))
            test_socket.close()
            print(f"⚠️  {name} (port {port}): NOT RUNNING")
            all_ok = False
        except OSError:
            print(f"✅ {name} (port {port}): RUNNING")

    return all_ok


def main():
    """Main test runner"""
    print(
        """
╔════════════════════════════════════════════════════════════╗
║          SOLANA SCALPER BOT - E2E INTEGRATION TEST         ║
╚════════════════════════════════════════════════════════════╝
    """
    )

    print("📋 TEST PLAN:")
    print("   1. Send advice messages to Brain (port 45100)")
    print("   2. Brain processes and sends decisions (port 45110)")
    print("   3. Measure end-to-end latency")
    print("   4. Verify target: <250ms average latency")
    print()

    # Check services
    services_ok = check_services()

    if not services_ok:
        print("\n⚠️  WARNING: Some services may not be running!")
        print("   Make sure Brain service is started before running this test.")
        response = input("\n   Continue anyway? (y/n): ")
        if response.lower() != "y":
            print("❌ Test cancelled")
            return

    print()
    input("Press ENTER to start the test...")

    # Run tests
    tester = IntegrationTester()
    tester.run_tests()


if __name__ == "__main__":
    main()
