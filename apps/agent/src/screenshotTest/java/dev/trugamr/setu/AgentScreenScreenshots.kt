package dev.trugamr.setu

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import com.android.tools.screenshot.PreviewTest

@PreviewTest
@Preview(name = "Sending", widthDp = 390, heightDp = 844)
@Composable
fun SendingShot() = SendingPreview()

@PreviewTest
@Preview(name = "Ready", widthDp = 390, heightDp = 844)
@Composable
fun ReadyShot() = ReadyPreview()

@PreviewTest
@Preview(name = "Off", widthDp = 390, heightDp = 844)
@Composable
fun OffShot() = OffPreview()

@PreviewTest
@Preview(name = "NotSetUp", widthDp = 390, heightDp = 844)
@Composable
fun NotSetUpShot() = NotSetUpPreview()

@PreviewTest
@Preview(name = "Panel", widthDp = 1024, heightDp = 640)
@Composable
fun PanelShot() = PanelPreview()
