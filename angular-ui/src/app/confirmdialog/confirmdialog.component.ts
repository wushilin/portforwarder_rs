import { MatDialogModule, MatDialogRef } from '@angular/material/dialog';
import { MatButtonModule } from '@angular/material/button';
import { Component, OnInit, Inject } from '@angular/core';
import { MAT_DIALOG_DATA } from '@angular/material/dialog';
@Component({
    selector: 'dialog-animations-example-dialog',
    templateUrl: 'confirm.html'
})
export class ConfirmationDialogComponent {
    message: string = "selected entry"
    confirmButtonText = "Confirm"
    cancelButtonText = "Cancel"
    title = "Confirmation"
    constructor(
        @Inject(MAT_DIALOG_DATA) private data: any,
        public dialogRef: MatDialogRef<ConfirmationDialogComponent>) {
        if (data) {
            this.message = data.message || this.message;
            if (data.buttonText) {
                this.confirmButtonText = data.buttonText.ok || this.confirmButtonText;
                this.cancelButtonText = data.buttonText.cancel || this.cancelButtonText;
            }
            this.title = data.title || this.title;
        }
    }

    onConfirmClick(): void {
        this.dialogRef.close(true);
    }
}