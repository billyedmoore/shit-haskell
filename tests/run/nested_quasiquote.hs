-- A quasi-quotation inside a splice runs first (path's `$(TH.lift ([OsPath.pstr|.|] <> ..))`).
{-# LANGUAGE TemplateHaskell, QuasiQuotes #-}
import qualified Q
import Language.Haskell.TH.Syntax (lift)

dotted :: String
dotted = $(lift ([Q.str|.|] ++ "/" ++ [Q.str|x|]))

shout :: String
shout = $(lift ([Q.fn|!|] "hi"))

main :: IO ()
main = putStrLn dotted >> putStrLn shout
