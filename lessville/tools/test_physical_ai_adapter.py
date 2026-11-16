#!/usr/bin/env python3
"""Regression checks for swarm-safe physical envelope lineage."""

import unittest

from physical_ai_adapter import adapt_event


class SwarmLineageTests(unittest.TestCase):
    def event(self, sensor_id: str) -> dict:
        return {
            "episode_id": "episode:swarm-test",
            "timestamp": "sequence-7",
            "sequence": 7,
            "sensor_id": sensor_id,
            "kind": "swarm_pose_observed",
            "frame_id": "sim:floor",
            "pose_mm": {"x": 0, "y": 0, "z": 120},
            "uncertainty": {"radius_mm": 5, "confidence_ppm": 970000},
            "action_kind": "walk",
        }

    def test_same_tick_agents_do_not_collapse(self) -> None:
        first = adapt_event("microduck", self.event("microduck:duck-1:pose"))
        second = adapt_event("microduck", self.event("microduck:duck-2:pose"))
        self.assertNotEqual(
            first["observation"]["observation_id"],
            second["observation"]["observation_id"],
        )
        self.assertNotEqual(first["proposal"]["proposal_id"], second["proposal"]["proposal_id"])
        self.assertNotEqual(
            first["proposal"]["idempotency_key"],
            second["proposal"]["idempotency_key"],
        )


if __name__ == "__main__":
    unittest.main()
