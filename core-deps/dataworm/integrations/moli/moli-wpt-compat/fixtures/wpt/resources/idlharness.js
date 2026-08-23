(function (global) {
  "use strict";

  const WELL_KNOWN_IDL_SOURCES = {
    "webcrypto": `
      interface Crypto {
        readonly attribute SubtleCrypto subtle;
        ArrayBufferView getRandomValues(ArrayBufferView array);
        DOMString randomUUID();
      };

      interface CryptoKey {
        readonly attribute DOMString type;
        readonly attribute boolean extractable;
        readonly attribute object algorithm;
        readonly attribute object usages;
      };

      interface SubtleCrypto {
        Promise<ArrayBuffer> encrypt(object algorithm, CryptoKey key, BufferSource data);
        Promise<ArrayBuffer> decrypt(object algorithm, CryptoKey key, BufferSource data);
        Promise<ArrayBuffer> sign(object algorithm, CryptoKey key, BufferSource data);
        Promise<boolean> verify(object algorithm, CryptoKey key, BufferSource signature, BufferSource data);
        Promise<ArrayBuffer> digest(object algorithm, BufferSource data);
        Promise<object> generateKey(object algorithm, boolean extractable, sequence<DOMString> keyUsages);
        Promise<CryptoKey> deriveKey(object algorithm, CryptoKey baseKey, object derivedKeyType, boolean extractable, sequence<DOMString> keyUsages);
        Promise<ArrayBuffer> deriveBits(object algorithm, CryptoKey baseKey, optional unsigned long length);
        Promise<CryptoKey> importKey(DOMString format, object keyData, object algorithm, boolean extractable, sequence<DOMString> keyUsages);
        Promise<object> exportKey(DOMString format, CryptoKey key);
        Promise<ArrayBuffer> wrapKey(DOMString format, CryptoKey key, CryptoKey wrappingKey, object wrapAlgorithm);
        Promise<CryptoKey> unwrapKey(DOMString format, BufferSource wrappedKey, CryptoKey unwrappingKey, object unwrapAlgorithm, object unwrappedKeyAlgorithm, boolean extractable, sequence<DOMString> keyUsages);
      };
    `,
    "hr-time": `
      typedef double DOMHighResTimeStamp;
      interface Performance : EventTarget {
        readonly attribute DOMHighResTimeStamp timeOrigin;
        readonly attribute object timing;
        readonly attribute object navigation;
        DOMHighResTimeStamp now();
        object toJSON();
      };
    `,
  };

  class IdlArray {
    constructor() {
      this.interfaces = [];
      this.objects = {};
      this.verifyFunctionLengths = false;
      this.verifyInterfaceDescriptors = false;
      this.verifyOperationDescriptors = false;
      this.verifyAttributeDescriptors = false;
      this.verifyObjectBrand = false;
    }

    add_idls(idl) {
      this.interfaces.push(...global.WebIDLParser.parse(idl));
    }

    add_objects(objects) {
      Object.assign(this.objects, objects);
    }

    enable_function_length_assertions() {
      this.verifyFunctionLengths = true;
    }

    enable_interface_descriptor_assertions() {
      this.verifyInterfaceDescriptors = true;
    }

    assert_interface_object(iface) {
      const name = iface.name;
      const constructor = global[name];
      assert_equals(typeof constructor, "function", name + " constructor should exist");
      assert_equals(typeof constructor.prototype, "object", name + " prototype should exist");

      if (this.verifyFunctionLengths && iface.constructors.length > 0) {
        const length = Math.min(...iface.constructors.map((entry) => entry.length));
        assert_equals(constructor.length, length, name + " constructor length");
      }

      if (this.verifyInterfaceDescriptors) {
        const descriptor = Object.getOwnPropertyDescriptor(global, name);
        assert_true(!!descriptor, name + " should have an own global descriptor");
        assert_equals(descriptor.value, constructor, name + " global descriptor value");
        assert_equals(descriptor.writable, true, name + " global descriptor writable");
        assert_equals(descriptor.enumerable, false, name + " global descriptor enumerable");
        assert_equals(descriptor.configurable, true, name + " global descriptor configurable");
      }
    }

    enable_operation_descriptor_assertions() {
      this.verifyOperationDescriptors = true;
    }

    enable_attribute_descriptor_assertions() {
      this.verifyAttributeDescriptors = true;
    }

    enable_object_brand_assertions() {
      this.verifyObjectBrand = true;
    }

    assert_operation(target, operation, label) {
      const value = target[operation.name];
      assert_equals(typeof value, "function", label + " should be a function");
      if (this.verifyFunctionLengths) {
        assert_equals(value.length, operation.length, label + " length");
      }
      if (this.verifyOperationDescriptors) {
        const descriptor = Object.getOwnPropertyDescriptor(target, operation.name);
        assert_true(!!descriptor, label + " should have an own property descriptor");
        assert_equals(descriptor.value, value, label + " descriptor value");
        assert_equals(
          descriptor.writable,
          !operation.unforgeable,
          label + " writable should match unforgeable shape",
        );
        assert_equals(descriptor.enumerable, true, label + " should be enumerable");
        assert_equals(
          descriptor.configurable,
          !operation.unforgeable,
          label + " configurable should match unforgeable shape",
        );
      }
    }

    assert_attribute(target, attribute, label) {
      if (!this.verifyAttributeDescriptors) {
        assert_true(attribute.name in target, label + " attribute should exist");
        return;
      }
      const descriptor = Object.getOwnPropertyDescriptor(target, attribute.name);
      assert_true(!!descriptor, label + " should have an own property descriptor");
      assert_equals(typeof descriptor.get, "function", label + " getter");
      if (attribute.readonly) {
        assert_equals(descriptor.set, undefined, label + " readonly setter");
      } else {
        assert_equals(typeof descriptor.set, "function", label + " setter");
      }
      assert_equals(descriptor.enumerable, true, label + " should be enumerable");
      assert_equals(
        descriptor.configurable,
        !attribute.unforgeable,
        label + " configurable should match unforgeable shape",
      );
    }

    test() {
      for (const iface of this.interfaces) {
        const name = iface.name;
        const objectExpressions = this.objects[name] || [];
        test(() => {
          this.assert_interface_object(iface);
        }, name + " interface object and prototype");

        if (iface.parent) {
          test(function () {
            assert_equals(
              typeof global[iface.parent],
              "function",
              iface.parent + " parent constructor should exist",
            );
            assert_equals(
              Object.getPrototypeOf(global[name].prototype),
              global[iface.parent].prototype,
              name + " prototype should inherit from " + iface.parent + ".prototype",
            );
          }, name + " inherits from " + iface.parent);
        }

        for (const operation of iface.operations) {
          if (operation.unforgeable) {
            test(() => {
              assert_true(
                objectExpressions.length > 0,
                name + "." + operation.name + " needs an instance object",
              );
              for (const expression of objectExpressions) {
                const object = global.eval(expression);
                this.assert_operation(
                  object,
                  operation,
                  expression + "." + operation.name,
                );
              }
            }, name + " object has unforgeable operation " + operation.name);
          } else {
            test(() => {
              this.assert_operation(
                global[name].prototype,
                operation,
                name + "." + operation.name,
              );
            }, name + " prototype has operation " + operation.name);
          }
        }

        for (const operation of iface.staticOperations) {
          test(() => {
            this.assert_operation(global[name], operation, name + "." + operation.name);
          }, name + " interface object has static operation " + operation.name);
        }

        for (const attribute of iface.attributes) {
          test(() => {
            if (attribute.unforgeable) {
              assert_true(
                objectExpressions.length > 0,
                name + "." + attribute.name + " needs an instance object",
              );
              for (const expression of objectExpressions) {
                const object = global.eval(expression);
                this.assert_attribute(
                  object,
                  attribute,
                  expression + "." + attribute.name,
                );
              }
              return;
            }
            if (this.verifyAttributeDescriptors) {
              this.assert_attribute(
                global[name].prototype,
                attribute,
                name + "." + attribute.name,
              );
              return;
            }
            const hasPrototypeAttribute = attribute.name in global[name].prototype;
            const hasObjectAttribute = objectExpressions.some(function (expression) {
              const object = global.eval(expression);
              return attribute.name in object;
            });
            assert_true(
              hasPrototypeAttribute || hasObjectAttribute,
              name + "." + attribute.name + " attribute should exist",
            );
          }, name + " has attribute " + attribute.name);
        }

        if (iface.stringifier) {
          if (iface.stringifierAttribute && objectExpressions.length > 0) {
            test(function () {
              for (const expression of objectExpressions) {
                const object = global.eval(expression);
                const expected = String(object[iface.stringifierAttribute]);
                assert_equals(
                  typeof object.toString,
                  "function",
                  expression + " stringifier should expose toString",
                );
                assert_equals(
                  object.toString(),
                  expected,
                  expression +
                    " stringifier should match " +
                    iface.stringifierAttribute,
                );
                assert_equals(
                  String(object),
                  expected,
                  expression +
                    " String() should match " +
                    iface.stringifierAttribute,
                );
              }
            }, name + " stringifier tracks " + iface.stringifierAttribute);
          } else {
            test(function () {
              const hasPrototypeStringifier =
                typeof global[name].prototype.toString === "function";
              const hasObjectStringifier = objectExpressions.some(function (expression) {
                const object = global.eval(expression);
                return typeof object.toString === "function";
              });
              assert_true(
                hasPrototypeStringifier || hasObjectStringifier,
                name + " stringifier should expose toString",
              );
            }, name + " has stringifier");
          }
        }

        for (const expression of objectExpressions) {
          test(function () {
            const object = global.eval(expression);
            assert_true(object instanceof global[name], expression + " should create " + name);
          }, expression + " creates " + name + " instance");

          if (this.verifyObjectBrand) {
            test(function () {
              const object = global.eval(expression);
              assert_equals(
                Object.prototype.toString.call(object),
                "[object " + name + "]",
                expression + " object brand",
              );
            }, expression + " has " + name + " object brand");
          }
        }
      }
    }
  }

  global.IdlArray = IdlArray;
  global.idl_test = function (idlSources, _dependencies, callback) {
    const idlArray = new IdlArray();
    for (const idl of idlSources) {
      idlArray.add_idls(WELL_KNOWN_IDL_SOURCES[idl] || idl);
    }
    callback(idlArray);
    idlArray.test();
    return Promise.resolve();
  };
})(globalThis);
