class BaseAnswer {
  constructor() {
    this.kind = "class";
  }
}

export class DerivedAnswer extends BaseAnswer {
  constructor() {
    super();
    this.value = "named-export-ok";
  }
}
