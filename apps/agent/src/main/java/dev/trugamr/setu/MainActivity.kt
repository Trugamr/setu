package dev.trugamr.setu

import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val canWriteSecureSettings =
            checkSelfPermission(Manifest.permission.WRITE_SECURE_SETTINGS) == PackageManager.PERMISSION_GRANTED
        val agent = Agent(LoopbackServer(), LocalAdb(this), PreferencesStore(this), { headsetAddress(this) })
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) { agent.run() }
        }
        // Setup's last step: the PC opens adbd's TCP port, then starts the agent with this extra.
        if (savedInstanceState == null && intent.getBooleanExtra(EXTRA_APPROVE, false)) {
            lifecycleScope.launch { agent.approve() }
        }
        setContent {
            SetuTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    val state by agent.state.collectAsStateWithLifecycle()
                    AgentScreen(
                        state = state,
                        canWriteSecureSettings = canWriteSecureSettings,
                        onStart = { lifecycleScope.launch { agent.startServer() } },
                        onStop = { lifecycleScope.launch { agent.stopServer() } },
                        onVbanChange = agent::editVban,
                    )
                }
            }
        }
    }
}

/** Set by setup (`am start --ez dev.trugamr.setu.APPROVE true`) to have the headset approve the agent's adb key. */
const val EXTRA_APPROVE = "dev.trugamr.setu.APPROVE"
