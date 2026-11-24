{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family T @k (a :: k) :: Type where
  T a = Bool
main :: IO ()
main = pure ()
