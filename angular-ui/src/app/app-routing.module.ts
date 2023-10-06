import { NgModule } from '@angular/core';
import { RouterModule, Routes, UrlSegment} from '@angular/router';
import { ConfigurationComponent } from "./configuration/configuration.component";
import { AppComponent} from "./app.component";

const routes: Routes = [
  { path: '', component: AppComponent },
  ];

@NgModule({
  imports: [RouterModule.forRoot(routes)],
  exports: [RouterModule]
})
export class AppRoutingModule { }
