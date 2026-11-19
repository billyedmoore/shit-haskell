-- Template Haskell queries a splice may make (text-show's reifyFixity and reifyRoles; getQ and
-- putQ; isExtEnabled).
{-# LANGUAGE TemplateHaskell, ScopedTypeVariables #-}
import Language.Haskell.TH
import Language.Haskell.TH.Syntax (putQ, getQ)
import Ops

main :: IO ()
main = do
  putStrLn $(do
    f1 <- reifyFixity '(<+>)
    f2 <- reifyFixity '(:*:)
    f3 <- reifyFixity 'plain
    pure (LitE (StringL (show (f1, f2, f3)))))
  putStrLn $(do
    rs <- reifyRoles ''Pair
    pure (LitE (StringL (show (length rs)))))
  putStrLn $(do
    on <- isExtEnabled ScopedTypeVariables
    off <- isExtEnabled Arrows
    pure (LitE (StringL (show (on, off)))))
  putStrLn $(do
    putQ (42 :: Int)
    putQ ("text" :: String)
    n <- getQ :: Q (Maybe Int)
    s <- getQ :: Q (Maybe String)
    b <- getQ :: Q (Maybe Bool)
    pure (LitE (StringL (show (n, s, b)))))
