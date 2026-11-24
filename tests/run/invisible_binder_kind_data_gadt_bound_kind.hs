{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs,DataKinds,GADTs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
data T @(k :: Type) :: k -> Type where
  T :: T a
main :: IO ()
main = pure ()
