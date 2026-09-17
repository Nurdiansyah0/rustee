package com.invinite.pwa

import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.webkit.ConsoleMessage
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.ProgressBar
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat

/**
 * MainActivity serves as the native Android host container embedding the Vue 3 PWA via Android WebView.
 *
 * SPECIFICATION CONFORMANCE:
 * - Master Specification v3.1.0 § 28 (Android Native Shell) & § 29 (JS Bridge)
 * - Safe area insets and Edge-to-Edge display support.
 * - Hardware accelerated WebView with modern security policies (mixed content blocked, file access disabled).
 * - Injects versioned InviniteBridge as `window.InviniteBridge`.
 * - Intercepts back navigation to prioritize WebView history navigation.
 * - Coordinates background synchronization via WorkManager.
 *
 * NATIVE LAYER INVARIANT:
 * The native Android layer must NEVER own financial calculations, subscription authority,
 * or ledger persistence. All financial logic belongs exclusively to the backend.
 */
class MainActivity : AppCompatActivity() {

    companion object {
        private const val TAG = "MainActivity"

        // Default application URL
        const val DEFAULT_APP_URL = "https://api.nurdiansyahlabs.com"

        // Local development URLs for emulator testing
        const val EMULATOR_DEV_URL = "http://10.0.2.2:5173"
        const val LOCALHOST_DEV_URL = "http://localhost:5173"
    }

    private lateinit var webView: WebView
    private lateinit var progressBar: ProgressBar
    private lateinit var secureStorage: SecureStorage
    private lateinit var inviniteBridge: InviniteBridge

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // 1. Enable Edge-to-Edge display
        WindowCompat.setDecorFitsSystemWindows(window, false)

        setContentView(R.layout.activity_main)

        // 2. Initialize Safe Area Insets
        setupSafeAreaInsets()

        // 3. Initialize Secure Storage & JS Bridge
        secureStorage = SecureStorage(this)
        webView = findViewById(R.id.webView)
        progressBar = findViewById(R.id.progressBar)

        inviniteBridge = InviniteBridge(this, webView, secureStorage)

        // 4. Configure WebView Settings
        configureWebView()

        // 5. Setup Back Navigation Dispatcher
        setupBackNavigation()

        // 6. Schedule Periodic WorkManager Sync
        BackgroundSyncWorker.schedulePeriodicSync(applicationContext)

        // 7. Load Initial Target Application URL
        val targetUrl = determineInitialUrl()
        Log.i(TAG, "Loading target application URL: $targetUrl")
        webView.loadUrl(targetUrl)
    }

    /**
     * Applies system window insets (status bar, navigation bar, display cutout) for edge-to-edge rendering.
     */
    private fun setupSafeAreaInsets() {
        val rootLayout = findViewById<View>(R.id.rootLayout)
        ViewCompat.setOnApplyWindowInsetsListener(rootLayout) { view, windowInsets ->
            val insets = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            // Apply top and bottom insets to ensure content respects physical cutouts and bars
            view.setPadding(insets.left, insets.top, insets.right, insets.bottom)
            WindowInsetsCompat.CONSUMED
        }
    }

    /**
     * Configures the WebView with modern security and performance parameters.
     */
    @SuppressLint("SetJavaScriptEnabled")
    private fun configureWebView() {
        // Hardware acceleration
        webView.setLayerType(View.LAYER_TYPE_HARDWARE, null)

        val settings = webView.settings
        // Core features required by Vue 3 PWA
        settings.javaScriptEnabled = true
        settings.domStorageEnabled = true
        settings.databaseEnabled = true

        // Viewport & Scaling
        settings.useWideViewPort = true
        settings.loadWithOverviewMode = true
        settings.setSupportZoom(false)
        settings.builtInZoomControls = false
        settings.displayZoomControls = false

        // Security constraints
        settings.allowFileAccess = false
        settings.allowContentAccess = false
        settings.mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
        settings.cacheMode = WebSettings.LOAD_DEFAULT

        // Modern media playback
        settings.mediaPlaybackRequiresUserGesture = false

        // Inject JavaScript Capability Bridge
        webView.addJavascriptInterface(inviniteBridge, "InviniteBridge")

        // Configure Clients
        webView.webViewClient = InviniteWebViewClient()
        webView.webChromeClient = InviniteWebChromeClient()
    }

    /**
     * Handles system back navigation in WebView history before exiting activity.
     */
    private fun setupBackNavigation() {
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                if (webView.canGoBack()) {
                    webView.goBack()
                } else {
                    isEnabled = false
                    onBackPressedDispatcher.onBackPressed()
                }
            }
        })
    }

    /**
     * Determines initial URL to load.
     */
    private fun determineInitialUrl(): String {
        return intent?.dataString ?: DEFAULT_APP_URL
    }

    /**
     * Custom WebViewClient enforcing origin security and external link delegation.
     */
    private inner class InviniteWebViewClient : WebViewClient() {

        override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
            val url = request?.url?.toString() ?: return false

            // Allow navigation inside authorized application origins
            if (InviniteBridge.isAuthorizedOrigin(url)) {
                return false
            }

            // Open external URLs (e.g. payment portals, help pages) in system browser
            return try {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                startActivity(intent)
                true
            } catch (e: Exception) {
                Log.e(TAG, "Failed to launch external browser for URL: $url", e)
                false
            }
        }

        override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
            super.onPageStarted(view, url, favicon)
            progressBar.visibility = View.VISIBLE
            progressBar.progress = 10
        }

        override fun onPageFinished(view: WebView?, url: String?) {
            super.onPageFinished(view, url)
            progressBar.visibility = View.GONE
        }
    }

    /**
     * Custom WebChromeClient managing page load progress.
     */
    private inner class InviniteWebChromeClient : WebChromeClient() {
        override fun onProgressChanged(view: WebView?, newProgress: Int) {
            super.onProgressChanged(view, newProgress)
            if (newProgress < 100) {
                progressBar.visibility = View.VISIBLE
                progressBar.progress = newProgress
            } else {
                progressBar.visibility = View.GONE
            }
        }

        override fun onConsoleMessage(consoleMessage: ConsoleMessage?): Boolean {
            if (consoleMessage != null) {
                Log.d("InvinitePWA-JS", "[${consoleMessage.messageLevel()}] ${consoleMessage.message()} -- line ${consoleMessage.lineNumber()}")
            }
            return super.onConsoleMessage(consoleMessage)
        }
    }

    override fun onResume() {
        super.onResume()
        webView.onResume()
    }

    override fun onPause() {
        super.onPause()
        webView.onPause()
    }

    override fun onDestroy() {
        webView.destroy()
        super.onDestroy()
    }
}
