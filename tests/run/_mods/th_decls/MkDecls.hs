{-# LANGUAGE TemplateHaskell #-}
module MkDecls (mkConst, mkEnum, mkAlias, mkShowName) where

import Language.Haskell.TH

-- | `name :: Integer; name = value`
mkConst :: String -> Integer -> Q [Dec]
mkConst name value = do
  let n = mkName name
  pure [SigD n (ConT ''Integer), ValD (VarP n) (NormalB (LitE (IntegerL value))) []]

-- | `data T = C1 | C2 .. deriving (Show, Eq, Enum, Bounded)`
mkEnum :: String -> [String] -> Q [Dec]
mkEnum ty cons =
  pure [DataD [] (mkName ty) [] Nothing [NormalC (mkName c) [] | c <- cons]
          [DerivClause Nothing [ConT ''Show, ConT ''Eq, ConT ''Enum, ConT ''Bounded]]]

-- | `new = old`, given the name of a definition made earlier in the module.
mkAlias :: Name -> String -> Q [Dec]
mkAlias old new = do
  body <- [| map succ $(varE old) |]
  pure [ValD (VarP (mkName new)) (NormalB body) []]

-- | A function naming a type given by a name quote: `name :: String`.
mkShowName :: Name -> String -> Q [Dec]
mkShowName t fn = pure [FunD (mkName fn) [Clause [] (NormalB (LitE (StringL (nameBase t)))) []]]
