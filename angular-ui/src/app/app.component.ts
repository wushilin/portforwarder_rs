import { Component, OnInit } from '@angular/core';
import {MatTabsModule} from '@angular/material/tabs';
import {RouterModule} from "@angular/router";
import { ConfigurationComponent} from "./configuration/configuration.component";
import { ThemePalette } from '@angular/material/core';
import { MaterialLoaderModule } from './materialloader/materialloader.module';
import { MatTabGroup } from '@angular/material/tabs';
@Component({
  selector: 'app-root',
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.scss'],
})
export class AppComponent implements OnInit {
  activeBG:ThemePalette= "primary"
  inactiveBG:ThemePalette = "accent"
  activeLink:string = ""
  links:string[] = ["link1", "link2", "link3"]
  title = 'ykcui';
  construct(home:ConfigurationComponent) {
    
  }
  
  ngOnInit(): void {
    console.log("AppComponent init")
  }

  select(link:string) {
    
  }
}
