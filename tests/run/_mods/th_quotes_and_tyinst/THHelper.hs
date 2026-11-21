{-# LANGUAGE TemplateHaskell, TypeFamilies #-}
module THHelper where

import Language.Haskell.TH

class Tagged a where
  type TagOf a
  tagVal :: a -> TagOf a

_hiddenVal :: Int
_hiddenVal = 7

mkTaggedInst :: Name -> Q Type -> Q Exp -> Q [Dec]
mkTaggedInst tyName tagTy valExp = do
  t <- [t| Either $tagTy $(conT ''Bool) |]
  e <- [|\x -> Left ($valExp x)|]
  pure
    [ InstanceD Nothing [] (AppT (ConT ''Tagged) (ConT tyName))
        [ TySynInstD (TySynEqn Nothing (AppT (ConT ''TagOf) (ConT tyName)) t)
        , PragmaD (InlineP 'tagVal Inline FunLike AllPhases)
        , FunD 'tagVal [Clause [] (NormalB e) []]
        ]
    ]

mkNewType :: String -> Q [Dec]
mkNewType s = do
  let n = mkName s
  pure [DataD [] n [] Nothing [NormalC n [(Bang NoSourceUnpackedness NoSourceStrictness, ConT ''Int)]] []]

describeBuiltins :: Name -> Name -> Q Exp
describeBuiltins _tyName conName = do
  TyConI (DataD _ _ _ _ cs0 _) <- reify ''()
  TyConI (DataD _ _ tvs2 _ cs2 _) <- reify ''(,)
  DataConI _ _ _ <- reify '()
  DataConI _ _ _ <- reify '(,)
  stricts <- reifyConStrictness conName
  let s = show (length cs0, length tvs2, length cs2, stricts, nameBase '_hiddenVal)
  [|s|]
