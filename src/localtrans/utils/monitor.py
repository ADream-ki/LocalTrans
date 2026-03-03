"""
性能监控模块
实现延迟监控和性能统计
"""

import time
import threading
from typing import Dict, List, Optional, Callable
from dataclasses import dataclass, field
from collections import deque
from enum import Enum

import numpy as np
from loguru import logger


class MetricType(Enum):
    """指标类型"""
    LATENCY = "latency"
    THROUGHPUT = "throughput"
    ERROR_RATE = "error_rate"
    RESOURCE = "resource"


@dataclass
class MetricRecord:
    """指标记录"""
    timestamp: float
    value: float
    tag: Optional[str] = None


@dataclass
class LatencyStats:
    """延迟统计"""
    min_ms: float = 0.0
    max_ms: float = 0.0
    avg_ms: float = 0.0
    p50_ms: float = 0.0
    p95_ms: float = 0.0
    p99_ms: float = 0.0
    count: int = 0
    
    def to_dict(self) -> Dict:
        return {
            "min_ms": round(self.min_ms, 2),
            "max_ms": round(self.max_ms, 2),
            "avg_ms": round(self.avg_ms, 2),
            "p50_ms": round(self.p50_ms, 2),
            "p95_ms": round(self.p95_ms, 2),
            "p99_ms": round(self.p99_ms, 2),
            "count": self.count,
        }


class PerformanceMonitor:
    """
    性能监控器
    实时监控流水线性能指标
    """
    
    def __init__(
        self,
        history_size: int = 1000,
        report_interval: float = 5.0,
        callback: Optional[Callable[[Dict], None]] = None,
    ):
        self.history_size = history_size
        self.report_interval = report_interval
        self.callback = callback
        
        # 指标存储
        self._metrics: Dict[str, deque] = {}
        self._counters: Dict[str, int] = {}
        self._start_times: Dict[str, float] = {}
        
        # 锁
        self._lock = threading.Lock()
        
        # 报告线程
        self._report_thread: Optional[threading.Thread] = None
        self._running = False
        
        logger.info("PerformanceMonitor初始化完成")
    
    def start_timer(self, name: str) -> None:
        """开始计时"""
        with self._lock:
            self._start_times[name] = time.perf_counter()
    
    def stop_timer(self, name: str) -> float:
        """停止计时并记录"""
        with self._lock:
            if name not in self._start_times:
                return 0.0
            
            elapsed = (time.perf_counter() - self._start_times[name]) * 1000  # ms
            del self._start_times[name]
            
            self._record(f"{name}_latency", elapsed)
            return elapsed
    
    def record(self, name: str, value: float, tag: str = None) -> None:
        """记录指标值"""
        with self._lock:
            self._record(name, value, tag)
    
    def _record(self, name: str, value: float, tag: str = None) -> None:
        """内部记录方法"""
        if name not in self._metrics:
            self._metrics[name] = deque(maxlen=self.history_size)
        
        self._metrics[name].append(MetricRecord(
            timestamp=time.time(),
            value=value,
            tag=tag,
        ))
    
    def increment(self, name: str, delta: int = 1) -> None:
        """增加计数器"""
        with self._lock:
            self._counters[name] = self._counters.get(name, 0) + delta
    
    def get_counter(self, name: str) -> int:
        """获取计数器值"""
        return self._counters.get(name, 0)
    
    def get_latency_stats(self, name: str) -> LatencyStats:
        """获取延迟统计"""
        metric_name = f"{name}_latency"
        
        with self._lock:
            if metric_name not in self._metrics or not self._metrics[metric_name]:
                return LatencyStats()
            
            values = [r.value for r in self._metrics[metric_name]]
        
        values = np.array(values)
        
        return LatencyStats(
            min_ms=float(np.min(values)),
            max_ms=float(np.max(values)),
            avg_ms=float(np.mean(values)),
            p50_ms=float(np.percentile(values, 50)),
            p95_ms=float(np.percentile(values, 95)),
            p99_ms=float(np.percentile(values, 99)),
            count=len(values),
        )
    
    def get_stats(self, name: str) -> Dict:
        """获取指标统计"""
        with self._lock:
            if name not in self._metrics or not self._metrics[name]:
                return {}
            
            values = [r.value for r in self._metrics[name]]
        
        values = np.array(values)
        
        return {
            "min": float(np.min(values)),
            "max": float(np.max(values)),
            "mean": float(np.mean(values)),
            "std": float(np.std(values)),
            "count": len(values),
        }
    
    def get_report(self) -> Dict:
        """获取完整报告"""
        report = {
            "timestamp": time.time(),
            "counters": dict(self._counters),
            "latencies": {},
            "metrics": {},
        }
        
        # 延迟统计
        for name in self._metrics:
            if name.endswith("_latency"):
                short_name = name.replace("_latency", "")
                report["latencies"][short_name] = self.get_latency_stats(short_name).to_dict()
            else:
                report["metrics"][name] = self.get_stats(name)
        
        return report
    
    def _report_loop(self) -> None:
        """报告循环"""
        while self._running:
            time.sleep(self.report_interval)
            
            if self.callback and self._running:
                report = self.get_report()
                try:
                    self.callback(report)
                except Exception as e:
                    logger.error(f"报告回调错误: {e}")
    
    def start(self) -> None:
        """启动监控"""
        if self._running:
            return
        
        self._running = True
        
        if self.callback:
            self._report_thread = threading.Thread(
                target=self._report_loop,
                daemon=True,
            )
            self._report_thread.start()
        
        logger.info("性能监控已启动")
    
    def stop(self) -> None:
        """停止监控"""
        self._running = False
        
        if self._report_thread:
            self._report_thread.join(timeout=2.0)
            self._report_thread = None
        
        logger.info("性能监控已停止")
    
    def reset(self) -> None:
        """重置所有指标"""
        with self._lock:
            self._metrics.clear()
            self._counters.clear()
            self._start_times.clear()


class LatencyTracker:
    """
    延迟追踪器
    追踪端到端延迟
    """
    
    def __init__(self):
        self._stages: Dict[str, float] = {}
        self._lock = threading.Lock()
        self._pipeline_start: Optional[float] = None
    
    def start_pipeline(self) -> None:
        """开始流水线追踪"""
        with self._lock:
            self._pipeline_start = time.perf_counter()
            self._stages.clear()
    
    def record_stage(self, stage: str) -> float:
        """记录阶段时间"""
        with self._lock:
            now = time.perf_counter()
            elapsed = (now - self._pipeline_start) * 1000 if self._pipeline_start else 0
            self._stages[stage] = elapsed
            return elapsed
    
    def end_pipeline(self) -> Dict[str, float]:
        """结束流水线追踪"""
        with self._lock:
            total = (time.perf_counter() - self._pipeline_start) * 1000 if self._pipeline_start else 0
            self._stages["total"] = total
            return dict(self._stages)
    
    def get_breakdown(self) -> Dict[str, float]:
        """获取延迟分解"""
        with self._lock:
            result = {}
            prev_time = 0.0
            
            for stage, elapsed in sorted(self._stages.items(), key=lambda x: x[1]):
                if stage != "total":
                    result[f"{stage}_duration"] = elapsed - prev_time
                    prev_time = elapsed
            
            result["total"] = self._stages.get("total", 0)
            return result


class ResourceMonitor:
    """
    资源监控器
    监控CPU、内存使用
    """
    
    def __init__(self, interval: float = 1.0):
        self.interval = interval
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._cpu_usage: deque = deque(maxlen=60)
        self._memory_usage: deque = deque(maxlen=60)
    
    def _monitor_loop(self) -> None:
        """监控循环"""
        import psutil
        
        process = psutil.Process()
        
        while self._running:
            try:
                cpu = process.cpu_percent()
                memory = process.memory_info().rss / 1024 / 1024  # MB
                
                self._cpu_usage.append(cpu)
                self._memory_usage.append(memory)
                
            except Exception as e:
                logger.error(f"资源监控错误: {e}")
            
            time.sleep(self.interval)
    
    def start(self) -> None:
        """启动监控"""
        if self._running:
            return
        
        try:
            import psutil
            self._running = True
            self._thread = threading.Thread(target=self._monitor_loop, daemon=True)
            self._thread.start()
            logger.info("资源监控已启动")
        except ImportError:
            logger.warning("psutil未安装，资源监控不可用")
    
    def stop(self) -> None:
        """停止监控"""
        self._running = False
        if self._thread:
            self._thread.join(timeout=2.0)
    
    def get_stats(self) -> Dict:
        """获取资源统计"""
        if not self._cpu_usage or not self._memory_usage:
            return {}
        
        return {
            "cpu_percent": {
                "current": self._cpu_usage[-1] if self._cpu_usage else 0,
                "avg": np.mean(self._cpu_usage),
            },
            "memory_mb": {
                "current": self._memory_usage[-1] if self._memory_usage else 0,
                "avg": np.mean(self._memory_usage),
            },
        }
