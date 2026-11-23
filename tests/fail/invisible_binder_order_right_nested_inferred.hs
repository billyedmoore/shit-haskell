{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, TypeOperators, RankNTypes #-}
import Data.Kind (Type)
type T :: Type -> forall {k}. k -> Type
data T a @k b = MkT
main :: IO ()
main = pure ()
