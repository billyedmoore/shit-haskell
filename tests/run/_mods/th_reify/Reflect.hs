{-# LANGUAGE TemplateHaskell #-}
module Reflect (conNames, mkGetters, arity) where

import Language.Haskell.TH

-- | The names of a type's constructors, as a list of strings.
conNames :: Name -> Q Exp
conNames t = do
  info <- reify t
  case info of
    TyConI (DataD _ _ _ _ cons _) -> listE [stringE (nameBase (conName c)) | c <- cons]
    TyConI (NewtypeD _ _ _ _ c _) -> listE [stringE (nameBase (conName c))]
    _ -> fail "not a data type"
  where
    conName (NormalC n _) = n
    conName (RecC n _) = n
    conName (InfixC _ n _) = n
    conName _ = mkName "?"

-- | `get_f = f` for each field `f` of a record type (lens-style generated accessors).
mkGetters :: Name -> Q [Dec]
mkGetters t = do
  TyConI (DataD _ _ _ _ [RecC _ fields] _) <- reify t
  pure [ FunD (mkName ("get_" ++ nameBase f)) [Clause [] (NormalB (VarE f)) []] | (f, _, _) <- fields ]

-- | The number of fields of a constructor, from its type.
arity :: Name -> Q Exp
arity c = do
  DataConI _ ty _ <- reify c
  let count (ForallT _ _ t) = count t
      count (AppT (AppT ArrowT _) r) = 1 + count r
      count _ = 0 :: Integer
  litE (integerL (count ty))
