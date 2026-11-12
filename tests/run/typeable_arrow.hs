{-# LANGUAGE TypeApplications, GADTs, PatternSynonyms, ScopedTypeVariables, PolyKinds, DataKinds #-}
-- An unsaturated arrow's representation is FUN 'Many with its runtime representations, the
-- same as splitting a function type gives (binary's TypeRep round trip rebuilds them).
module Main where

import Data.Binary (decode, encode)
import qualified Data.Typeable as T
import Data.Kind (Type)
import Type.Reflection
import Type.Reflection.Unsafe (mkTrApp, typeRepFingerprint)

main :: IO ()
main = do
  let r = typeRep @(() -> ())
  case r of
    App f x -> do
      case f of
        App p y -> do
          print (p, y, typeRepTyCon p == typeRepTyCon (typeRep @(->)))
          print (typeRepTyCon p, typeRepKind y, typeRepKind y == typeRep @Type)
          print (SomeTypeRep (typeRep @((->) ())) == SomeTypeRep f)
          print (typeRepFingerprint (typeRep @((->) ())), typeRepFingerprint f)
          print (SomeTypeRep (mkTrApp f x) == SomeTypeRep r, mkTrApp f x)
          print (SomeTypeRep (mkTrApp p y) == SomeTypeRep f)
        _ -> putStrLn "f not app"
      print (typeRepKind f, typeRepKind x)
    _ -> putStrLn "not app"
  let reps = [T.typeRep (T.Proxy :: T.Proxy (() -> ())), T.typeRep (T.Proxy :: T.Proxy (Maybe Int)), T.typeRep (T.Proxy :: T.Proxy (Int -> [Bool]))]
  print (map (\t -> decode (encode t) :: T.TypeRep) reps, map (\t -> decode (encode t) == t) reps)
