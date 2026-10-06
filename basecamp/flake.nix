{
  description = "Logos Names for Basecamp: look up, message and pay a .lez name";

  nixConfig = {
    extra-substituters = [ "https://cache.nix.logos.co/public" ];
    extra-trusted-public-keys = [ "public:l4HrXgL4nw246+LBh2SOJyhz64BoGegOYLheT/iIAPU=" ];
  };

  inputs = {
    # The builder revision Basecamp 0.3.1 itself locks, so this package is built
    # the same way as the apps that release ships with.
    logos-module-builder.url = "github:logos-co/logos-module-builder/de169fd45d7bdf6e28ce1f151b4c34880523ace9";
    # The lookup module this view is a front end for. The input name must match
    # the dependency name in metadata.json. When building from a copy that is not
    # a git checkout, pass `--override-input names path:/abs/path/to/basecamp-module`.
    names.url = "path:../basecamp-module";
    names.inputs.logos-module-builder.follows = "logos-module-builder";
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosQmlModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
    };
}
