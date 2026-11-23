{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, RankNTypes #-}
import Data.Kind (Type)
type T :: forall k. k -> *
data T a @k = T
main :: IO ()
main = pure ()
