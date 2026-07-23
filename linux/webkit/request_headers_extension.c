#include <glib.h>
#include <libsoup/soup.h>
#include <string.h>
#include <webkit/webkit-web-process-extension.h>

static gboolean cmux_send_request(WebKitWebPage *page,
                                  WebKitURIRequest *request,
                                  WebKitURIResponse *redirected_response,
                                  gpointer user_data) {
    const gchar *directory = g_getenv("CMUX_WEBKIT_REQUEST_CONFIG_DIR");
    if (!directory || !*directory)
        return FALSE;

    gchar *filename = g_strdup_printf("%" G_GUINT64_FORMAT ".headers",
                                      webkit_web_page_get_id(page));
    gchar *path = g_build_filename(directory, filename, NULL);
    gchar *contents = NULL;
    gsize length = 0;
    if (g_file_get_contents(path, &contents, &length, NULL)) {
        SoupMessageHeaders *headers = webkit_uri_request_get_http_headers(request);
        gsize offset = 0;
        while (headers && offset < length) {
            const gchar *name = contents + offset;
            gsize name_length = strnlen(name, length - offset);
            if (name_length == length - offset)
                break;
            offset += name_length + 1;
            const gchar *value = contents + offset;
            gsize value_length = strnlen(value, length - offset);
            if (value_length == length - offset)
                break;
            offset += value_length + 1;
            if (name_length)
                soup_message_headers_replace(headers, name, value);
        }
    }
    g_free(contents);
    g_free(path);
    g_free(filename);
    return FALSE;
}

static void cmux_page_created(WebKitWebProcessExtension *extension,
                              WebKitWebPage *page,
                              gpointer user_data) {
    g_signal_connect(page, "send-request", G_CALLBACK(cmux_send_request), NULL);
}

G_MODULE_EXPORT void webkit_web_process_extension_initialize(
    WebKitWebProcessExtension *extension) {
    g_signal_connect(extension, "page-created", G_CALLBACK(cmux_page_created), NULL);
}
