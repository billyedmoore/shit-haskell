{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, RankNTypes #-}
import Data.Kind (Type)
type K = forall (k :: Type). Type -> Type
type T :: K
data T @k @j a = T
main :: IO ()
main = pure ()
