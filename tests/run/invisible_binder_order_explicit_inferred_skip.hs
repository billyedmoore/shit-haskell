{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, TypeOperators, RankNTypes #-}
import Data.Kind (Type)
type T :: forall {k} (j :: Type). k -> j -> Type
data T @j a b = MkT
main :: IO ()
main = pure ()
