from dataclasses import dataclass
from typing import List, Optional

@dataclass
class BlackSeg:
    start: float
    end: float

    @property
    def dur(self) -> float:
        return max(0.0, self.end - self.start)

@dataclass
class CutInterval:
    start: float
    end: float
    kind: str  # "commercial" or "keep"

    @property
    def dur(self) -> float:
        return max(0.0, self.end - self.start)

@dataclass
class Plan:
    blacks: List[BlackSeg]
    commercials: List[CutInterval]
    keeps: List[CutInterval]
    include_black: bool
    edge_pad_pre: float
    edge_pad_post: float
    params: dict
    duration: Optional[float] = None
