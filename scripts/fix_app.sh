#!/bin/bash

APP_PATH="/Applications/Antigravity Tools.app"

echo "🛠️  Fixing 'Antigravity Tools' damaged issue..."

if [ -d "$APP_PATH" ]; then
    echo "📍 Application found: $APP_PATH"
    echo "🔑 Administrator privileges are required to remove the quarantine attribute..."
    
    sudo xattr -rd com.apple.quarantine "$APP_PATH"
    
    if [ $? -eq 0 ]; then
        echo "✅ Fix successful! You should now be able to open the application normally."
    else
        echo "❌ Fix failed, please check your password or permissions."
    fi
else
    echo "⚠️  Application not found. Please ensure the application is installed in '/Applications'."
    echo "   If installed elsewhere, please run manually: sudo xattr -rd com.apple.quarantine /path/to/app"
fi
