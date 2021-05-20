CREATE TABLE swag_inventory_items (
    id SERIAL PRIMARY KEY,
    name VARCHAR NOT NULL,
    size VARCHAR NOT NULL DEFAULT 'N/A',
    current_stock INTEGER NOT NULL,
    item VARCHAR NOT NULL,
    barcode VARCHAR NOT NULL,
    barcode_png VARCHAR NOT NULL,
    barcode_svg VARCHAR NOT NULL,
    barcode_pdf_label VARCHAR NOT NULL,
    print_barcode_label BOOLEAN NOT NULL DEFAULT 'f',
    print_barcode_label_quantity INTEGER NOT NULL DEFAULT 1,
    link_to_item TEXT [] NOT NULL,
    airtable_record_id VARCHAR NOT NULL DEFAULT ''
)
