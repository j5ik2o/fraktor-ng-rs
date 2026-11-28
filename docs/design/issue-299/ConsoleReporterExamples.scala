// Drives the original PekkoConsoleReporter (Pekko v2.0.0, compiled from its
// source by derive-console-reporter-examples.sh) and prints the report of each
// case of docs/design/issue-183-metrics-kit.md §2. Every case starts with a
// line "=== <name>" and the report follows byte for byte; the script compares
// the whole output with console-reporter-examples.expected.
//
// Run with -Dline.separator=$'\r\n' and the argument "crlf" to see the bytes
// the reporter writes on Windows: that mode prints one case with CR shown as
// "<CR>".
// The package is inside org.apache.pekko because the metrics classes are
// private[pekko].
package org.apache.pekko.testkit.metrics.examples

import java.io.{ ByteArrayOutputStream, PrintStream }
import java.lang.management.OperatingSystemMXBean
import java.lang.reflect.{ InvocationHandler, Method, Proxy }

import com.codahale.metrics.{ Gauge, MetricRegistry }
import com.sun.management.UnixOperatingSystemMXBean

import org.apache.pekko.testkit.metrics._
import org.apache.pekko.testkit.metrics.reporter.PekkoConsoleReporter

object ConsoleReporterExamples {

  private def newRegistry() = new MetricRegistry() with PekkoMetricRegistry

  private def report(registry: MetricRegistry with PekkoMetricRegistry, verbose: Boolean): String = {
    val bytes = new ByteArrayOutputStream()
    val output = new PrintStream(bytes, true, "UTF-8")
    val reporter = new PekkoConsoleReporter(registry, verbose, output)
    try reporter.report()
    catch { case e: Throwable => output.print(s"<thrown: ${e.getClass.getName}>") }
    output.flush()
    bytes.toString("UTF-8")
  }

  private def emit(name: String, text: String): Unit = {
    print(s"=== $name\n")
    print(text)
  }

  // An OS bean with fixed file descriptor counts, as the original's
  // FileDescriptorMetricSet(os) constructor takes one.
  private def unixBean(open: Long, max: Long): OperatingSystemMXBean =
    Proxy
      .newProxyInstance(
        getClass.getClassLoader,
        Array(classOf[UnixOperatingSystemMXBean]),
        new InvocationHandler {
          def invoke(p: AnyRef, m: Method, args: Array[AnyRef]): AnyRef = m.getName match {
            case "getOpenFileDescriptorCount" => java.lang.Long.valueOf(open)
            case "getMaxFileDescriptorCount"  => java.lang.Long.valueOf(max)
            case other                        => throw new UnsupportedOperationException(other)
          }
        })
      .asInstanceOf[OperatingSystemMXBean]

  // An OS bean that is not a UnixOperatingSystemMXBean, as on Windows.
  private def nonUnixBean(): OperatingSystemMXBean =
    Proxy
      .newProxyInstance(
        getClass.getClassLoader,
        Array(classOf[OperatingSystemMXBean]),
        new InvocationHandler {
          def invoke(p: AnyRef, m: Method, args: Array[AnyRef]): AnyRef =
            throw new UnsupportedOperationException(m.getName)
        })
      .asInstanceOf[OperatingSystemMXBean]

  // MetricsKitOps.measureFileDescriptors registers the set under
  // key / "file-descriptors"; MetricKey sanitizes the "." of each name to "-".
  private def registerFileDescriptors(registry: MetricRegistry, os: OperatingSystemMXBean): Unit = {
    val metrics = new FileDescriptorMetricSet(os).getMetrics
    metrics.forEach { (name, metric) =>
      registry.register("metrics-kit.file-desc.file-descriptors." + name.replace(".", "-"), metric)
    }
  }

  private def gauge(sum: Long, count: Int): AveragingGauge = {
    val g = new AveragingGauge
    if (count > 0) {
      g.add(Seq.fill(count - 1)(0L))
      g.add(sum)
    }
    g
  }

  // A timer whose elapsed time is fixed; avgDuration and opsPerSecond are the
  // original's own definitions over elapsedTime.
  private def timer(ops: Long, elapsedNanos: Long): KnownOpsInTimespanTimer =
    new KnownOpsInTimespanTimer(ops) { override def elapsedTime: Long = elapsedNanos }

  private def histogram(values: Seq[Long]): HdrHistogram = {
    val h = new HdrHistogram(100000000L, 4, "ns")
    values.foreach(h.update)
    h
  }

  def main(args: Array[String]): Unit = {
    if (args.headOption.contains("crlf")) {
      val r = newRegistry()
      r.register("metrics-kit.hist.hdr-histogram", histogram(0L to 4L))
      emit("crlf: HdrHistogram 0..4, verbose", report(r, verbose = true).replace("\r", "<CR>"))
      return
    }

    emit("empty registry", report(newRegistry(), verbose = false))

    {
      val r = newRegistry()
      val avg = new AveragingGauge
      avg.add(Seq(1L, 2L, 3L))
      avg.add(4L)
      r.register("metrics-kit.avg-size", avg)
      registerFileDescriptors(r, unixBean(42L, 1024L))
      emit("gauges: averaging gauge and file descriptors 42/1024", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      registerFileDescriptors(r, unixBean(42L, 1024L))
      emit("gauges: file descriptors only", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      registerFileDescriptors(r, unixBean(42L, 0L))
      emit("gauges: file descriptors with limit 0", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      registerFileDescriptors(r, nonUnixBean())
      emit("gauges: file descriptors on a non-unix bean", report(r, verbose = false))
      print("\n")
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.file-desc.file-descriptors.file-descriptors-ratio",
        new FileDescriptorMetricSet(nonUnixBean()).getMetrics.get("file-descriptors.ratio"))
      emit("gauges: the ratio alone on a non-unix bean", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.a-empty", gauge(0, 0))
      r.register("metrics-kit.b-tiny", gauge(1, 10000))
      r.register("metrics-kit.c-quarter", gauge(25, 4))
      r.register("metrics-kit.d-whole", gauge(4, 2))
      r.register("metrics-kit.e-eighth", gauge(1, 8))
      emit("averaging gauges: Double.toString and HALF_UP", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.", gauge(1, 1))
      r.register("metrics-kit.😀", gauge(2, 1))
      emit("key order: U+E000 and U+1F600", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.t1-ms", timer(10L, 1234567L))
      r.register("metrics-kit.t2-zero", timer(10L, 0L))
      r.register("metrics-kit.t3-unstopped", timer(10L, 0L - 123456789012345L))
      r.register("metrics-kit.t4-no-ops", timer(0L, 1000L))
      r.register("metrics-kit.t5-no-ops-zero", timer(0L, 0L))
      emit("timers (the banner names the anonymous subclass of this driver)", report(r, verbose = false))
    }

    {
      // avgDuration of a negative elapsed time over 0 operations is
      // (-Infinity).toLong = Long.MinValue, which FiniteDuration rejects.
      val r = newRegistry()
      r.register("metrics-kit.t6-unstopped-no-ops", timer(0L, 0L - 123456789012345L))
      emit("a timer whose avg is Long.MinValue", report(r, verbose = false))
      print("\n")
    }

    {
      val r = newRegistry()
      r.register("metrics-kit", new KnownOpsInTimespanTimer(10L))
      val out = report(r, verbose = false)
      // An unstopped timer: elapsedTime = 0 - startTime, startTime = System.nanoTime,
      // so the digits depend on the clock of the machine.
      val shapes = out.linesIterator.map { line =>
        line
          .replaceAll("^( +time = )-[0-9.]+(e\\+[0-9]+)? (ns)$", "$1-<digits> $3")
          .replaceAll("^( +avg = )-[0-9.]+(e\\+[0-9]+)? (ns)$", "$1-<digits> $3")
          .replaceAll("^( +ops/s = )-[0-9.]+$", "$1-<digits>")
      }
      emit("an unstopped timer, digits masked", shapes.mkString("", "\n", "\n"))
    }

    {
      val r = newRegistry()
      val h = histogram(Seq.empty)
      (1 to 11).foreach(_ => (0L to 1579331L).foreach(h.update))
      h.update(1579331L)
      r.register("metrics-kit.hist.hdr-histogram", h)
      emit("HdrHistogram: case 4 input, not verbose", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.hist.hdr-histogram", histogram(0L to 4L))
      emit("HdrHistogram: 0..4, verbose", report(r, verbose = true))
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.hist.hdr-histogram", histogram(Seq.empty))
      emit("HdrHistogram: empty, verbose", report(r, verbose = true))
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.hist.hdr-histogram", histogram(Seq.fill(7)(1L) :+ 2L))
      emit("HdrHistogram: seven 1s and one 2", report(r, verbose = false))
    }

    {
      val r = newRegistry()
      r.register("metrics-kit.avg-size", gauge(10, 4))
      r.register("metrics-kit.hist.hdr-histogram", histogram(Seq(5L)))
      r.register("metrics-kit.timer", timer(10L, 1234567L))
      emit("kinds in order: gauges, timers, histograms, averaging gauges", report(r, verbose = false))
    }

    print("=== class names\n")
    val fd = new FileDescriptorMetricSet(unixBean(42L, 1024L)).getMetrics
    List("file-descriptors.open", "file-descriptors.max", "file-descriptors.ratio").foreach { name =>
      print(s"$name: ${fd.get(name).getClass.getName}\n")
    }
  }
}
