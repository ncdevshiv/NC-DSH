export default class NamedDefaultClass {
  constructor(value) {
    this.value = "class:" + value;
  }
}

export const localClassResult = new NamedDefaultClass("local").value;
