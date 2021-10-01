-- CreateTable
CREATE TABLE "rfd_sections" (
    "id" SERIAL NOT NULL,
    "anchor" TEXT NOT NULL,
    "content" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "rfds_id" INTEGER NOT NULL,

    PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "rfds.number_unique" ON "rfds"("number");

-- CreateIndex
CREATE UNIQUE INDEX "rfds.number_string_unique" ON "rfds"("number_string");

-- CreateIndex
CREATE UNIQUE INDEX "rfds.name_unique" ON "rfds"("name");

-- AddForeignKey
ALTER TABLE "sections" ADD FOREIGN KEY ("rfdsId") REFERENCES "rfds"("id") ON DELETE CASCADE ON UPDATE CASCADE
