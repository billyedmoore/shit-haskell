{-# LANGUAGE TypeFamilies, TypeFamilyDependencies, DataKinds, KindSignatures, FlexibleContexts, UndecidableInstances, UndecidableSuperClasses #-}
-- A constructor pattern inside a field of an injective family type whose argument is another
-- family stuck on a rigid variable (ghc-lib-parser's `HsOpTy _ prom ty1 (L _ op) ty2`): the bound
-- variable has the reduced type, so the context's constraints on it apply.
module Main where

import Data.Kind (Type)

data Located l e = L l e deriving Show

type family XRec p a = r | r -> a

data Pass = Parsed | Renamed

data GhcPass (c :: Pass) = GhcPass

type family Anno a :: Type

type instance XRec (GhcPass p) a = Located (Anno a) a

type family IdP p
type instance IdP (GhcPass p) = IdGhcP p

type family IdGhcP (p :: Pass) where
  IdGhcP 'Parsed = String
  IdGhcP 'Renamed = Int

type instance Anno String = ()
type instance Anno Int = Bool

class Show (IdGhcP p) => OutputableId p

instance OutputableId 'Parsed
instance OutputableId 'Renamed

type LIdP p = XRec p (IdP p)

data HsType pass = OpTy Int (LIdP pass) Int | VarTy (LIdP pass)

render :: OutputableId p => HsType (GhcPass p) -> String
render (OpTy a (L _ op) b) = unwords [show a, show op, show b]
render (VarTy (L _ v)) = show v

main :: IO ()
main = do
  putStrLn (render (OpTy 1 (L () "+") 2 :: HsType (GhcPass 'Parsed)))
  putStrLn (render (VarTy (L True 42) :: HsType (GhcPass 'Renamed)))
  print (depth (DocTy (L () (DocTy (L () (Leaf 5)))) :: Ty (GhcPass 'Parsed)))

-- A local recursive function without a signature matching inside the family-typed field: the
-- field's family is stuck on an unknown until the function's use fixes it.
data Ty pass = DocTy (XRec pass (Ty pass)) | Leaf Int

type instance Anno (Ty (GhcPass p)) = ()

depth :: Ty (GhcPass p) -> Int
depth = go
  where
    go (DocTy (L _ t)) = 1 + go t
    go (Leaf n) = n
