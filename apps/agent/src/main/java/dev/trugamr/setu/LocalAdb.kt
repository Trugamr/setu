package dev.trugamr.setu

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import dev.trugamr.setu.server.Server
import io.github.muntashirakon.adb.AbsAdbConnectionManager
import io.github.muntashirakon.adb.AdbAuthenticationFailedException
import io.github.muntashirakon.adb.AdbPairingRequiredException
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.TimeUnit

/**
 * The [HeadsetAdb] adapter: this headset's own adbd, reached with the agent's [AdbIdentity]
 * through libadb-android. Calls block, for up to [APPROVE_TIMEOUT_S] each time the headset asks.
 */
class LocalAdb(private val context: Context) : HeadsetAdb {

    private val identity by lazy { AdbIdentity.loadOrCreate(File(context.noBackupFilesDir, "adb")) }

    override fun approve(progress: (String) -> Unit): AdbResult {
        val deadline = System.nanoTime() + APPROVE_WINDOW_MS * 1_000_000
        var ask = "On the headset, choose “Always allow from this computer”."
        var asks = 0
        // Setup opens the agent before the PC opens the port, so wait for it.
        progress(WAITING_FOR_PC)
        while (true) {
            if (!awaitPort(LOOPBACK, LEGACY_PORT, PORT_WAIT_MS)) {
                return AdbResult.Failed("adb isn't listening on port $LEGACY_PORT. Run setup from a PC, which opens it.")
            }
            val adb = Connection(APPROVE_TIMEOUT_S)
            try {
                progress(ask)
                // adbd shows its "Allow USB debugging?" prompt for an unknown key and holds the
                // connection until it is answered.
                if (!adb.connect(LOOPBACK, LEGACY_PORT)) {
                    return AdbResult.Failed("The headset didn't allow Setu in time. Run setup again.")
                }
                // Allowed once, the key is forgotten when adbd restarts, and Start can't connect.
                // Reconnecting while the port is still open asks again.
                if (!isRemembered(adb)) {
                    if (++asks == MAX_ASKS) {
                        return AdbResult.Failed(
                            "The headset allowed Setu only once each time. Run setup again, and choose " +
                                "“Always allow from this computer”, not Allow.",
                        )
                    }
                    ask = "Allow lets Setu in only once. This time, choose “Always allow from this computer”."
                    continue
                }
                // Back to USB only, as `adb usb` does. adbd restarts, which ends this connection; an
                // installer watching `service.adb.tcp.port` read 0 knows setup finished.
                runCatching { adb.openStream("usb:") }
                return AdbResult.Done("This headset allows Setu. You can unplug the cable.")
            } catch (e: IOException) {
                // adbd restarted under the connection: a port left open by an earlier setup lets the
                // agent in before the PC's `adb tcpip` restarts adbd. Connect again; a key allowed
                // meanwhile is remembered, and the headset doesn't ask again.
                if (System.nanoTime() >= deadline) return AdbResult.Failed("Couldn't reach adb: ${e.message}")
                progress(WAITING_FOR_PC)
                Thread.sleep(PORT_POLL_MS)
            } finally {
                adb.disconnectQuietly()
            }
        }
    }

    /** Whether the headset remembers the agent's key, waiting briefly for it to be saved after Allow. */
    private fun isRemembered(adb: Connection): Boolean {
        val deadline = System.nanoTime() + REMEMBER_WAIT_MS * 1_000_000
        while (true) {
            if (identity.isRemembered(adb.shell("dumpsys adb"))) return true
            if (System.nanoTime() >= deadline) return false
            Thread.sleep(PORT_POLL_MS)
        }
    }

    /** Whether something accepts connections on [host]:[port] within [timeoutMs]. */
    private fun awaitPort(host: String, port: Int, timeoutMs: Long): Boolean {
        val deadline = System.nanoTime() + timeoutMs * 1_000_000
        while (true) {
            try {
                Socket().use { it.connect(InetSocketAddress(host, port), PORT_POLL_MS.toInt()) }
                return true
            } catch (e: IOException) {
                if (System.nanoTime() >= deadline) return false
                Thread.sleep(PORT_POLL_MS)
            }
        }
    }

    override fun launchServer(progress: (String) -> Unit): AdbResult {
        if (context.checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS) != PackageManager.PERMISSION_GRANTED) {
            return AdbResult.Failed(NOT_SET_UP)
        }
        val resolver = context.contentResolver
        val wasOn = Settings.Global.getInt(resolver, ADB_WIFI_ENABLED, 0) == 1
        if (!wasOn) Settings.Global.putInt(resolver, ADB_WIFI_ENABLED, 1)
        val adb = Connection(CONNECT_TIMEOUT_S)
        return try {
            progress("Connecting to wireless debugging. If the headset asks to allow it on this network, choose Allow.")
            if (!adb.connectTls(context, DISCOVERY_TIMEOUT_MS)) {
                return AdbResult.Failed("adb didn't answer. Try again.")
            }
            progress("Launching the capture server…")
            // The shell exits once the server is in the background, which closes the stream.
            adb.shell(launchCommand())
            AdbResult.Done()
        } catch (e: InterruptedException) {
            AdbResult.Failed(
                "Wireless debugging didn't turn on. If the headset asked to allow it on this network, " +
                    "choose Allow, then try again.",
            )
        } catch (e: AdbPairingRequiredException) {
            AdbResult.Failed(NOT_SET_UP)
        } catch (e: AdbAuthenticationFailedException) {
            AdbResult.Failed(NOT_SET_UP)
        } catch (e: IOException) {
            AdbResult.Failed("Couldn't reach adb: ${e.message}")
        } finally {
            adb.disconnectQuietly()
            // The server is detached from adbd, so it keeps running with wireless debugging off.
            if (!wasOn) Settings.Global.putInt(resolver, ADB_WIFI_ENABLED, 0)
        }
    }

    /**
     * Runs the server from this APK as the shell user, detached from adbd (`setsid nohup`) so it
     * outlives the connection.
     */
    private fun launchCommand(): String {
        val apk = context.applicationInfo.sourceDir
        // adbd ends the shell's process group when the connection closes, which kills a server that
        // hasn't left it yet, so the shell returns only once the server leads its own session.
        return "CLASSPATH='$apk' setsid nohup app_process / ${Server::class.java.name} udp " +
            ">$SERVER_LOG_PATH 2>&1 </dev/null & pid=\$!; " +
            "while [ -d /proc/\$pid ] && [ \"\$(cut -d' ' -f6 /proc/\$pid/stat)\" != \"\$pid\" ]; do sleep 0.01; done"
    }

    private inner class Connection(timeoutSeconds: Long) : AbsAdbConnectionManager() {
        init {
            setApi(Build.VERSION.SDK_INT)
            setTimeout(timeoutSeconds, TimeUnit.SECONDS)
            setThrowOnUnauthorised(false)
        }

        override fun getPrivateKey() = identity.privateKey

        override fun getCertificate() = identity.certificate

        override fun getDeviceName() = "Setu agent"

        /** Runs [command] in a shell and returns what it printed, once the shell exits. */
        fun shell(command: String): String = openStream("shell:$command").use { stream ->
            val out = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val read = try {
                    stream.read(buffer, 0, buffer.size)
                } catch (e: IOException) {
                    // libadb reports the end of a stream that closes while a read waits as
                    // "Stream closed.", with everything before it already read.
                    if (stream.isClosed) break else throw e
                }
                if (read < 0) break
                out.write(buffer, 0, read)
            }
            out.toString(Charsets.UTF_8.name())
        }

        fun disconnectQuietly() {
            // disconnect(), not close(): close() destroys the private key, which later connections need.
            runCatching { disconnect() }
        }
    }

    private companion object {
        const val LOOPBACK = "127.0.0.1"
        /** The port setup opens with `adb tcpip 5555`. */
        const val LEGACY_PORT = 5555
        const val ADB_WIFI_ENABLED = "adb_wifi_enabled"
        const val APPROVE_TIMEOUT_S = 60L
        /** How many times the headset asks before setup gives up on "Always allow". */
        const val MAX_ASKS = 3
        /** How long setup may keep reconnecting after adbd restarts under it. */
        const val APPROVE_WINDOW_MS = 180_000L
        /** How long after Allow the headset has to save the key. */
        const val REMEMBER_WAIT_MS = 2_000L
        /** How long setup's PC has to open [LEGACY_PORT] after starting the agent. */
        const val PORT_WAIT_MS = 30_000L
        const val PORT_POLL_MS = 250L
        private const val WAITING_FOR_PC = "Waiting for the PC to finish setup…"
        const val CONNECT_TIMEOUT_S = 10L
        /** How long to look for adbd's TLS port over mDNS, including the headset's allow prompt. */
        const val DISCOVERY_TIMEOUT_MS = 30_000L
        const val NOT_SET_UP = "This headset hasn't allowed Setu yet. Run setup from a PC."
    }
}
